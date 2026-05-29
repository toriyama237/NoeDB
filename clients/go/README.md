# noedb-go

Go gRPC client for [NoeDB](https://github.com/toriyama237/NoeDB) v2.

```go
package main

import (
	"fmt"
	"log"

	"github.com/toriyama237/noedb-go/noedb"
)

func main() {
	c, err := noedb.ConnectDevCerts("127.0.0.1:5434", "/tmp/noedb-dev", 1, "noedb-dev")
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
	fmt.Println(r.Rows)
}
```
