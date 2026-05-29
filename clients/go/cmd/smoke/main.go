package main

import (
	"flag"
	"fmt"
	"log"

	"github.com/toriyama237/noedb-go/noedb"
)

func main() {
	target := flag.String("target", "127.0.0.1:5434", "gRPC address")
	data := flag.String("data", "/tmp/noedb-dev", "data dir with dev TLS")
	flag.Parse()
	c, err := noedb.ConnectDevCerts(*target, *data, 1, "noedb-dev")
	if err != nil {
		log.Fatal(err)
	}
	defer c.Close()
	if err := c.Ping(); err != nil {
		log.Fatal(err)
	}
	r, err := c.Execute("SELECT 1")
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("go ok", r.Rows)
}
