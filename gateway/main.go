package main

import (
	"context"
	"flag"
	"net/http"
	"strings"

	"github.com/golang/glog"
	"github.com/grpc-ecosystem/grpc-gateway/v2/runtime"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/grpclog"

	auth "github.com/pulsar/pulsariumd/gateway/cosmos/auth/v1beta1"
	bank "github.com/pulsar/pulsariumd/gateway/cosmos/bank/v1beta1"
	tendermint "github.com/pulsar/pulsariumd/gateway/cosmos/base/tendermint/v1beta1"
	tx "github.com/pulsar/pulsariumd/gateway/cosmos/tx/v1beta1"
	cosmwasm "github.com/pulsar/pulsariumd/gateway/cosmwasm/wasm/v1"

	// to register pubkey any types
	_ "github.com/pulsar/pulsariumd/gateway/cosmos/crypto/ed25519"
	_ "github.com/pulsar/pulsariumd/gateway/cosmos/crypto/secp256k1"
)

var (
	// command-line options:
	// gRPC server endpoint
	grpcServerEndpoint = flag.String("grpc-server-endpoint", "localhost:9090", "gRPC server endpoint")
)

// Logic taken from the cosmos SDK code (server/api/server.go), so I guess it is needed
// We need to explicitly pass this header through (why? which client?)
const GRPCBlockHeightHeader = "x-cosmos-block-height"

func CustomGRPCHeaderMatcher(key string) (string, bool) {
	switch strings.ToLower(key) {
	case GRPCBlockHeightHeader:
		return GRPCBlockHeightHeader, true
	default:
		return runtime.DefaultHeaderMatcher(key)
	}
}

func run() error {
	ctx := context.Background()
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	// Create the grpc proxy mux, with custom header support
	mux := runtime.NewServeMux(
		runtime.WithIncomingHeaderMatcher(CustomGRPCHeaderMatcher),
	)

	endpoint := *grpcServerEndpoint
	opts := []grpc.DialOption{grpc.WithTransportCredentials(insecure.NewCredentials())}

	// create a connection to the underlying grpc server
	// Note: Make sure the gRPC server is running properly and accessible
	conn, err := grpc.DialContext(ctx, endpoint, opts...)
	if err != nil {
		return err
	}
	defer func() {
		if err != nil {
			if cerr := conn.Close(); cerr != nil {
				grpclog.Infof("Failed to close conn to %s: %v", endpoint, cerr)
			}
			return
		}
		go func() {
			<-ctx.Done()
			if cerr := conn.Close(); cerr != nil {
				grpclog.Infof("Failed to close conn to %s: %v", endpoint, cerr)
			}
		}()
	}()

	// Register the gateway handlers
	err = auth.RegisterQueryHandler(ctx, mux, conn)
	if err != nil {
		return err
	}
	err = bank.RegisterQueryHandler(ctx, mux, conn)
	if err != nil {
		return err
	}
	err = cosmwasm.RegisterQueryHandler(ctx, mux, conn)
	if err != nil {
		return err
	}
	err = tendermint.RegisterServiceHandler(ctx, mux, conn)
	if err != nil {
		return err
	}
	err = tx.RegisterServiceHandler(ctx, mux, conn)
	if err != nil {
		return err
	}

	// Start HTTP server (and proxy calls to gRPC server endpoint)
	return http.ListenAndServe(":1317", mux)
}

func main() {
	flag.Parse()
	defer glog.Flush()

	if err := run(); err != nil {
		glog.Fatal(err)
	}
}
