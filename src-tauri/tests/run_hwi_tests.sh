#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# HWI Integration Test Runner
# ============================================================================
#
# Usage:
#   ./tests/run_hwi_tests.sh                    # run all tests
#   ./tests/run_hwi_tests.sh discover            # run only discovery test
#   ./tests/run_hwi_tests.sh sign                # run only signing test
#   ./tests/run_hwi_tests.sh ledger              # run Ledger diagnostics
#   ./tests/run_hwi_tests.sh bitbox02            # run BitBox02 diagnostics
#   ./tests/run_hwi_tests.sh coldcard            # run Coldcard diagnostics
#   ./tests/run_hwi_tests.sh jade                # run Jade diagnostics
#   ./tests/run_hwi_tests.sh full                # run full E2E flow
#   ./tests/run_hwi_tests.sh xpub                # run xpub extraction
#   ./tests/run_hwi_tests.sh wallet              # run discovery with wallet config
#   ./tests/run_hwi_tests.sh unlock              # run discover + unlock
#
# ============================================================================

# ---------------------------------------------------------------------------
# Configuration - edit these to match your setup
# ---------------------------------------------------------------------------

# Network: "regtest", "testnet", "signet", "bitcoin"
export BITCOIN_NETWORK="${BITCOIN_NETWORK:-regtest}"

# Log level: "error", "warn", "info", "debug", "trace"
export RUST_LOG="${RUST_LOG:-info}"

# Base64-encoded PSBT for signing tests (leave empty to use built-in placeholder)
export TEST_PSBT="${TEST_PSBT:-}"

# Wallet config for policy registration tests
export TEST_WALLET_NAME="${TEST_WALLET_NAME:-test-wallet}"
export TEST_DESCRIPTOR="${TEST_DESCRIPTOR:-}"

# Ledger HMAC (hex-encoded 32 bytes, for Ledger wallet policy)
export TEST_HMAC="${TEST_HMAC:-}"


export BITCOIN_NETWORK=regtest
export TEST_DESCRIPTOR="tr([455537ce/84'/1'/0']tpubDD1r8bgxw1BSL4P8ovQEiy7V3NXnr9AiG2SXhhB1n2G6WFMBJ32yv6rsVHAWqgxrV115qbP83najbmRCUH7wQriMrxLtRcBGGyanFdq4Ydd/<0;1>/*,and_v(v:multi_a(2,[adb39d68/84'/1'/0']tpubDCX8rhtJtRW8cdAdPPXo3bfcMUqcyUySHfS8XGPb8MJRcC7VduQsnLLNrvPLwcXSb2rmMhoUASq9vbRH688KxNnz8dN1hjtUUTn2vDx9KVK/<0;1>/*,[432b9b68/84'/1'/0']tpubDD3gVvdsXEFE9aZfoR7QPFX2xUiCZeRKkgPim8YRh5ev3GzTK1rAY84HURoxZ9mNbjJccqQsK7B9ye3jpjS3HSwNFFRysZjP5Z9LDF56a1T/<0;1>/*,[59c5d866/84'/1'/0']tpubDC4CsCwcbpFzCeCPvE4L1PNNpFLCGduziFSrgRtNDMPPM87pERtT4mTFzfBsPPxN1h5ZZ8EYKzZo9tKMG4GwKiCqzmAagia5uJ6gfoQxHBq/<0;1>/*),older(10)))#tk6uwqnt"
export TEST_PSBT="cHNidP8BAIkCAAAAAb2JQsbQoJmVcCh4AtERUr9ZR4MTbdprj/yzWeyCrtsHAQAAAAAKAAAAAqCGAQAAAAAAIlEgEiiwaw0p8MthJqIhGzuVItI+A24gBopb7JNNqGTpYBsDug0AAAAAACJRIM8LSCfsdCA3Tlp1OJMPqifiu3qYS2icS/ip7t+l79cRyAAAAAABAStAQg8AAAAAACJRIDlxrTssQ9L1vXhmx/wDerP6CXhh7jDnpbrOhg7MpwawIhXAAoxfFzf6f6UwSonRNLo8pe9kbrHMzAlHVLpnCF9JhUlrIBtoYyoD7mh+vC2fyE+YD7KP6nVDCeJIPiTCFSMuDqV+rCDjby6NBk0I2h3LdJ7VvYXnH2I1ssW29qpCHONSoanzXbogf86MfzLwRrV7HN4KyH6SqJ3OncG0gXBNtkJUpeIidGK6Up1assAhFgKMXxc3+n+lMEqJ0TS6PKXvZG6xzMwJR1S6ZwhfSYVJGQBFVTfOVAAAgAEAAIAAAACAAAAAAAAAAAAhFhtoYyoD7mh+vC2fyE+YD7KP6nVDCeJIPiTCFSMuDqV+LQHHzAciXBvEuGLeHB+wmPkBM00p3SEhLWBEgijNRlM9e62znWgAAAAAAAAAACEWf86MfzLwRrV7HN4KyH6SqJ3OncG0gXBNtkJUpeIidGItAcfMByJcG8S4Yt4cH7CY+QEzTSndISEtYESCKM1GUz17WcXYZgAAAAAAAAAAIRbjby6NBk0I2h3LdJ7VvYXnH2I1ssW29qpCHONSoanzXS0Bx8wHIlwbxLhi3hwfsJj5ATNNKd0hIS1gRIIozUZTPXtDK5toAAAAAAAAAAABFyACjF8XN/p/pTBKidE0ujyl72RusczMCUdUumcIX0mFSQEYIMfMByJcG8S4Yt4cH7CY+QEzTSndISEtYESCKM1GUz17AAABBSDHaG4k/oy7s9rTFit0qGqUrNyogs9MWFLumxibSvXVZAEGbQDAaiBtQAE3bhGfFE9XV0qk3Nhdcw48rin74Plm4pBfpEtaT6wgLzz/lD7YcyaOYaADMiQLjFHCwpVowqN7y4WMKqiIGcG6IMGNy8JyjXTnzk+z4QQ5Jb3WMGTgHG4XCypavwrSzvuXulKdWrIhBy88/5Q+2HMmjmGgAzIkC4xRwsKVaMKje8uFjCqoiBnBLQHBSaJqI21UQjAqipaYBca8cHXQomC7nOeImRA6/YektkMrm2gBAAAAAAAAACEHbUABN24RnxRPV1dKpNzYXXMOPK4p++D5ZuKQX6RLWk8tAcFJomojbVRCMCqKlpgFxrxwddCiYLuc54iZEDr9h6S2rbOdaAEAAAAAAAAAIQfBjcvCco10585Ps+EEOSW91jBk4BxuFwsqWr8K0s77ly0BwUmiaiNtVEIwKoqWmAXGvHB10KJgu5zniJkQOv2HpLZZxdhmAQAAAAAAAAAhB8dobiT+jLuz2tMWK3SoapSs3KiCz0xYUu6bGJtK9dVkGQBFVTfOVAAAgAEAAIAAAACAAQAAAAAAAAAA"
TEST_WALLET_NAME="test3"
# ---------------------------------------------------------------------------
# Test name mapping
# ---------------------------------------------------------------------------

resolve_test_name() {
    case "${1:-all}" in
        all)        echo "" ;;
        discover)   echo "test_discover_devices" ;;
        sign)       echo "test_sign_psbt" ;;
        full)       echo "test_full_flow" ;;
        xpub)       echo "test_get_device_xpubs" ;;
        wallet)     echo "test_discover_with_wallet_config" ;;
        unlock)     echo "test_discover_and_unlock" ;;
        ledger)     echo "test_ledger_diagnostics" ;;
        bitbox02)   echo "test_bitbox02_diagnostics" ;;
        coldcard)   echo "test_coldcard_diagnostics" ;;
        jade)       echo "test_jade_diagnostics" ;;
        trezor)     echo "test_trezor_diagnostics" ;;
        *)          echo "$1" ;;  # pass through raw test name
    esac
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_FILTER=$(resolve_test_name "${1:-all}")

echo "========================================"
echo "  HWI Integration Tests"
echo "========================================"
echo "  Network:     $BITCOIN_NETWORK"
echo "  Log level:   $RUST_LOG"
echo "  Test:        ${1:-all}"
echo "  PSBT:        ${TEST_PSBT:+set (${#TEST_PSBT} chars)}${TEST_PSBT:-<default placeholder>}"
echo "  Wallet:      $TEST_WALLET_NAME"
echo "  Descriptor:  ${TEST_DESCRIPTOR:-<not set>}"
echo "========================================"
echo ""

cd "$SCRIPT_DIR"

cargo test --test hwi_integration ${TEST_FILTER:+$TEST_FILTER} -- --ignored --nocapture
