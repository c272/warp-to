#!/bin/bash
set -e

cargo build --release
mkdir -p build
cp -f ./target/release/warpto.exe ./build
cp -f ./scripts/* ./build
