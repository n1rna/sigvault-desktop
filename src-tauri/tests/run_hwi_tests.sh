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
export TEST_DESCRIPTOR="tr([adb39d68/84'/1'/0']tpubDCX8rhtJtRW8cdAdPPXo3bfcMUqcyUySHfS8XGPb8MJRcC7VduQsnLLNrvPLwcXSb2rmMhoUASq9vbRH688KxNnz8dN1hjtUUTn2vDx9KVK/<0;1>/*,and_v(v:pk([432b9b68]tpubDD3gVvdsXEFE9aZfoR7QPFX2xUiCZeRKkgPim8YRh5ev3GzTK1rAY84HURoxZ9mNbjJccqQsK7B9ye3jpjS3HSwNFFRysZjP5Z9LDF56a1T/<0;1>/*),older(12)))"
export TEST_PSBT="cHNidP8BAIkCAAAAAZeP3t50NEer/EyS3vRBkdw15NpLpt5eX8WHO2k76sD7AQAAAAD9////AkjDSgAAAAAAIlEgw0Ml1b2NQYwRFrSxigSsK3L9aZrW2LCeQtIU/1tzro2ghgEAAAAAACJRIBIosGsNKfDLYSaiIRs7lSLSPgNuIAaKW+yTTahk6WAbUAMAAAABAStAS0wAAAAAACJRIFVAJ2S51Cl0t95Ob72cVTF1RPk7owkA7W7b74kMEwc9IhXAG2hjKgPuaH68LZ/IT5gPso/qdUMJ4kg+JMIVIy4OpX4lIONvLo0GTQjaHct0ntW9hecfYjWyxbb2qkIc41KhqfNdrVyywCEWG2hjKgPuaH68LZ/IT5gPso/qdUMJ4kg+JMIVIy4OpX4NAK2znWgAAAAAAAAAACEW428ujQZNCNody3Se1b2F5x9iNbLFtvaqQhzjUqGp810tAW3AC4LNHR0iUq6cLJAjBKI11oEwrdgYGL3/p5MAzpXHQyubaAAAAAAAAAAAARcgG2hjKgPuaH68LZ/IT5gPso/qdUMJ4kg+JMIVIy4OpX4BGCBtwAuCzR0dIlKunCyQIwSiNdaBMK3YGBi9/6eTAM6VxwABBSBtQAE3bhGfFE9XV0qk3Nhdcw48rin74Plm4pBfpEtaTwEGJwDAJCAvPP+UPthzJo5hoAMyJAuMUcLClWjCo3vLhYwqqIgZwa1csiEHLzz/lD7YcyaOYaADMiQLjFHCwpVowqN7y4WMKqiIGcEtAUT+TPB2KTEFrGxipKkILSMKCUOkyQzZ3Sy7KykzibE5QyubaAEAAAAAAAAAIQdtQAE3bhGfFE9XV0qk3Nhdcw48rin74Plm4pBfpEtaTw0ArbOdaAEAAAAAAAAAAAA="
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
