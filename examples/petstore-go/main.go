// Example Go consumer of a specforge-generated Petstore SDK.
//
//	./scripts/generate-examples.sh
//	cd examples/petstore-go && go run .
package main

import (
	"context"
	"fmt"
	"os"
	"time"

	sdk "github.com/example/petstore-example-go"
)

func main() {
	base := env("PETSTORE_URL", "https://petstore3.swagger.io/api/v3")
	ctx := context.Background()

	c := sdk.NewClient().
		WithBaseURL(base).
		WithTimeout(10 * time.Second).
		WithMaxConcurrent(4).
		WithRetry(sdk.DefaultRetryOptions())

	// Observability middleware
	c.Use(func(ctx context.Context, req *sdk.MiddlewareRequest, next func(context.Context, *sdk.MiddlewareRequest) (*sdk.MiddlewareResponse, error)) (*sdk.MiddlewareResponse, error) {
		start := time.Now()
		res, err := next(ctx, req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "%s %s err=%v\n", req.Method, req.URL, err)
			return res, err
		}
		fmt.Fprintf(os.Stderr, "%s %s → %d (%s)\n", req.Method, req.URL, res.StatusCode, time.Since(start).Round(time.Millisecond))
		return res, err
	})

	pets, err := c.ListPets(ctx, 5)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ListPets failed (is the server up?): %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("fetched %d pets from %s\n", len(*pets), base)
	for i, p := range *pets {
		if i >= 3 {
			break
		}
		fmt.Printf("- %s (id=%v)\n", p.Name, p.Id)
	}
}

func env(k, def string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return def
}
