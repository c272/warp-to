$ErrorActionPreference = "Stop"

cargo build --release
New-Item -ItemType Directory -Force -Path build
cp ./target/release/warpto.exe ./build -Force
cp ./scripts/* ./build -Force
