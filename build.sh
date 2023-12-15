# Create output directory
mkdir -p output

# Get latest tags
git pull

# Get version number from tag name
export VERSION=$(git describe --tags $(git rev-list --tags --max-count=1))

if [ -n "$VERSION" ]; then
    # Confirm version number with user before build
    read -p "Build with version $VERSION? C-c to cancel, any key to continue " -n 1 -r
    echo

    # Update version number in Cargo.toml for build
    # macOS sed requires the weird empty string param
    # Otherwise it returns `invalid command code C`
    sed -i '' "s/version = \"0.0.0\"/version = \"$VERSION\"/g" imessage-database/Cargo.toml
    sed -i '' "s/version = \"0.0.0\"/version = \"$VERSION\"/g" imessage-exporter/Cargo.toml
    sed -i '' s/'{ path = "..\/imessage-database" }'/\"$VERSION\"/g imessage-exporter/Cargo.toml

    if [ -n "$PUBLISH" ]; then
        echo 'Publishing database library...'
        cargo publish -p imessage-database --allow-dirty

        echo 'Publishing exporter binary...'
        cargo publish -p imessage-exporter --allow-dirty
    else
        echo 'PUBLISH env var not set!'
    fi

    # Build for Apple Silicon
    cargo build --target aarch64-apple-darwin --release
    cp target/aarch64-apple-darwin/release/imessage-exporter output/imessage-exporter-aarch64-apple-darwin
    cd target/aarch64-apple-darwin/release
    tar -czf ../../../output/imessage-exporter-aarch64-apple-darwin.tar.gz imessage-exporter
    cd ../../..

    # Build for 64-bit Intel macOS
    cargo build --target x86_64-apple-darwin --release
    cp target/x86_64-apple-darwin/release/imessage-exporter output/imessage-exporter-x86_64-apple-darwin
    cd target/x86_64-apple-darwin/release/
    tar -czf ../../../output/imessage-exporter-x86_64-apple-darwin.tar.gz imessage-exporter
    cd ../../..

    # Put the version number back
    sed -i '' "s/version = \"$VERSION\"/version = \"0.0.0\"/g" imessage-database/Cargo.toml
    sed -i '' "s/version = \"$VERSION\"/version = \"0.0.0\"/g" imessage-exporter/Cargo.toml
    sed -i '' s/\"$VERSION\"/'{path = "..\/imessage-database"}'/g imessage-exporter/Cargo.toml

    unset VERSION
else
    echo 'No version tag set!'
fi
