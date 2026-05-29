package noedb

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"os"
	"path/filepath"

	noedbv1 "github.com/toriyama237/noedb-go/noedb/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/metadata"
)

const authMetadata = "x-noedb-auth"

// Client is a TLS gRPC SQL client.
type Client struct {
	conn   *grpc.ClientConn
	sql    noedbv1.SqlClient
	authHex string
}

// QueryResult is a tabular SQL result.
type QueryResult struct {
	Columns []string
	Rows    [][]string
}

// ConnectTLS dials `target` (e.g. "127.0.0.1:5434") with dev-style mTLS material.
func ConnectTLS(target string, caPEM, certPEM, keyPEM []byte, passphrase string) (*Client, error) {
	pool := x509.NewCertPool()
	if !pool.AppendCertsFromPEM(caPEM) {
		return nil, fmt.Errorf("noedb: invalid CA PEM")
	}
	cert, err := tls.X509KeyPair(certPEM, keyPEM)
	if err != nil {
		return nil, err
	}
	tlsCfg := &tls.Config{
		MinVersion:   tls.VersionTLS13,
		RootCAs:      pool,
		Certificates: []tls.Certificate{cert},
		ServerName:   "localhost",
	}
	conn, err := grpc.NewClient(target,
		grpc.WithTransportCredentials(credentials.NewTLS(tlsCfg)),
	)
	if err != nil {
		return nil, err
	}
	return &Client{
		conn:    conn,
		sql:     noedbv1.NewSqlClient(conn),
		authHex: ClusterAuthHex(passphrase),
	}, nil
}

// ConnectDevCerts loads PKI from a noedb-cli data directory.
func ConnectDevCerts(target, dataDir string, nodeID uint64, passphrase string) (*Client, error) {
	ca, err := os.ReadFile(filepath.Join(dataDir, "ca.pem"))
	if err != nil {
		return nil, err
	}
	cert, err := os.ReadFile(filepath.Join(dataDir, fmt.Sprintf("node-%d.pem", nodeID)))
	if err != nil {
		return nil, err
	}
	key, err := os.ReadFile(filepath.Join(dataDir, fmt.Sprintf("node-%d-key.pem", nodeID)))
	if err != nil {
		return nil, err
	}
	return ConnectTLS(target, ca, cert, key, passphrase)
}

func (c *Client) ctx() context.Context {
	return metadata.AppendToOutgoingContext(context.Background(), authMetadata, c.authHex)
}

// Ping checks server health.
func (c *Client) Ping() error {
	resp, err := c.sql.Ping(c.ctx(), &noedbv1.PingRequest{})
	if err != nil {
		return err
	}
	if _, ok := resp.Body.(*noedbv1.SqlResponse_Pong); !ok {
		return fmt.Errorf("noedb: unexpected ping response %T", resp.Body)
	}
	return nil
}

// Execute runs SQL.
func (c *Client) Execute(query string) (*QueryResult, error) {
	resp, err := c.sql.Execute(c.ctx(), &noedbv1.SqlRequest{Query: query})
	if err != nil {
		return nil, err
	}
	switch b := resp.Body.(type) {
	case *noedbv1.SqlResponse_Result:
		out := &QueryResult{Columns: append([]string(nil), b.Result.Columns...)}
		for _, row := range b.Result.Rows {
			out.Rows = append(out.Rows, append([]string(nil), row.Cells...))
		}
		return out, nil
	case *noedbv1.SqlResponse_Error:
		return nil, fmt.Errorf("noedb: %s", b.Error.Message)
	default:
		return nil, fmt.Errorf("noedb: unexpected response %T", resp.Body)
	}
}

// Close closes the connection.
func (c *Client) Close() error {
	return c.conn.Close()
}
