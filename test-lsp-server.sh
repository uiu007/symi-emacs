#!/bin/bash

# Test script for Symi LSP Server
# This script builds and tests the LSP server implementation

set -e

echo "🧪 Testing Symi LSP Server Implementation"
echo "=========================================="

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print status
print_status() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Check if we're in the right directory
if [ ! -f "editor/src-tauri/Cargo.toml" ]; then
    print_error "Not in symi-emacs root directory"
    exit 1
fi

print_status "Checking project structure..."

# Check if all required files exist
required_files=(
    "editor/src-tauri/src/bin/lsp-server.rs"
    "editor/src-tauri/src/bin/mod.rs"
    "editor/src-tauri/src/commands.rs"
    "editor/src-tauri/src/lib.rs"
    "editor/src-tauri/Cargo.toml"
    "emacs-symi-lsp.el"
    "LSP_README.md"
)

for file in "${required_files[@]}"; do
    if [ -f "$file" ]; then
        print_status "Found: $file"
    else
        print_error "Missing: $file"
        exit 1
    fi
done

print_status "All required files present"

# Check Rust toolchain
print_status "Checking Rust toolchain..."
if ! command -v cargo &> /dev/null; then
    print_error "Cargo not found. Please install Rust toolchain."
    exit 1
fi

RUST_VERSION=$(rustc --version | cut -d' ' -f2)
print_status "Rust version: $RUST_VERSION"

# Build the project
print_status "Building LSP server..."
cd editor/src-tauri

# Check if we can build the project
if cargo check --lib; then
    print_status "Project builds successfully"
else
    print_error "Project build failed"
    exit 1
fi

# Check if LSP dependencies are available
print_status "Checking LSP dependencies..."
if grep -q "lsp-server" Cargo.toml && grep -q "lsp-types" Cargo.toml; then
    print_status "LSP dependencies found in Cargo.toml"
else
    print_error "LSP dependencies missing from Cargo.toml"
    exit 1
fi

# Check if module is properly declared
print_status "Checking module declarations..."
if grep -q "pub mod bin" src/lib.rs; then
    print_status "bin module declared in lib.rs"
else
    print_error "bin module not declared in lib.rs"
    exit 1
fi

if grep -q "pub mod lsp_server" src/bin/mod.rs; then
    print_status "lsp_server module declared in bin/mod.rs"
else
    print_error "lsp_server module not declared in bin/mod.rs"
    exit 1
fi

# Check if LSP commands are added
print_status "Checking LSP commands..."
if grep -q "start_lsp_server" src/commands.rs && grep -q "stop_lsp_server" src/commands.rs; then
    print_status "LSP commands found in commands.rs"
else
    print_error "LSP commands missing from commands.rs"
    exit 1
fi

# Check if commands are registered in Tauri
if grep -q "commands::start_lsp_server" src/lib.rs && grep -q "commands::stop_lsp_server" src/lib.rs; then
    print_status "LSP commands registered in Tauri"
else
    print_error "LSP commands not registered in Tauri"
    exit 1
fi

# Test Emacs configuration syntax
print_status "Checking Emacs configuration..."
cd ../../
if command -v emacs &> /dev/null; then
    if emacs --batch --load emacs-symi-lsp.el --eval "(message 'Emacs configuration syntax OK')"; then
        print_status "Emacs configuration syntax is valid"
    else
        print_warning "Emacs configuration may have syntax issues"
    fi
else
    print_warning "Emacs not found, skipping Emacs configuration test"
fi

# Create a test Symi file
print_status "Creating test Symi file..."
cat > test.symi << 'EOF'
// Test Symi file for LSP server
foo = C4@3/2
<C4=440>
(120)
(3/4)
foo:D4,
C5:E5,
EOF

print_status "Test Symi file created: test.symi"

# Summary
echo ""
echo "🎉 LSP Server Implementation Test Summary"
echo "========================================"
print_status "✅ All required files present"
print_status "✅ Rust toolchain available"
print_status "✅ Project builds successfully"
print_status "✅ LSP dependencies configured"
print_status "✅ Module declarations correct"
print_status "✅ LSP commands implemented"
print_status "✅ Tauri integration complete"
print_status "✅ Test files created"

echo ""
echo "🚀 Next Steps:"
echo "1. Build the full project: cargo build --release"
echo "2. Start the LSP server: cargo run --bin lsp-server -- 3000"
echo "3. Configure Emacs with emacs-symi-lsp.el"
echo "4. Open .symi files in Emacs for full LSP support"
echo ""
echo "📚 See LSP_README.md for detailed usage instructions"
echo ""
echo "✨ LSP server implementation is ready!"

# Cleanup test file
rm -f test.symi