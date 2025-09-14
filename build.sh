#!/bin/bash

# Build and run IronBucket using Docker

echo "Building IronBucket..."

# Create necessary directories
mkdir -p s3

# Build the Docker image
docker build -t ironbucket:latest .

if [ $? -eq 0 ]; then
    echo "Build successful!"
    echo ""
    echo "To run IronBucket:"
    echo "  docker-compose up -d"
        echo ""
    echo "To use with AWS CLI:"
    echo "  aws s3 --endpoint-url http://localhost:20000 ls"
else
    echo "Build failed!"
    exit 1
fi