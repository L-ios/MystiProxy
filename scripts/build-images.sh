#!/bin/bash
# Build Docker images for MystiProxy Mock Management System
# Usage: ./scripts/build-images.sh [options]
#
# Options:
#   -t, --tag         Image tag (default: latest)
#   -r, --registry    Docker registry (default: none, local only)
#   -p, --push        Push images to registry after build
#   -h, --help        Show this help message

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
TAG="latest"
REGISTRY=""
PUSH=false
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--tag)
            TAG="$2"
            shift 2
            ;;
        -r|--registry)
            REGISTRY="$2"
            shift 2
            ;;
        -p|--push)
            PUSH=true
            shift
            ;;
        -h|--help)
            sed -n '2,/^$/p' "$0" | sed 's/^# //'
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Print banner
echo -e "${BLUE}"
echo "========================================"
echo "  MystiProxy Docker Image Builder"
echo "========================================"
echo -e "${NC}"

# Function to build image
build_image() {
    local name=$1
    local dockerfile_dir=$2
    local image_name="${name}:${TAG}"

    if [ -n "$REGISTRY" ]; then
        image_name="${REGISTRY}/${image_name}"
    fi

    echo -e "${YELLOW}Building ${name}...${NC}"
    echo -e "  Dockerfile: ${dockerfile_dir}/Dockerfile"
    echo -e "  Image: ${image_name}"

    if [ "$name" = "mysticentral" ]; then
        # Build mysticentral from project root
        docker build \
            -f "${dockerfile_dir}/Dockerfile" \
            -t "${image_name}" \
            "${PROJECT_ROOT}"
    else
        # Build other images from their directories
        docker build \
            -f "${dockerfile_dir}/Dockerfile" \
            -t "${image_name}" \
            "${dockerfile_dir}"
    fi

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}Successfully built ${image_name}${NC}"
    else
        echo -e "${RED}Failed to build ${name}${NC}"
        return 1
    fi

    # Push if requested
    if [ "$PUSH" = true ] && [ -n "$REGISTRY" ]; then
        echo -e "${YELLOW}Pushing ${image_name}...${NC}"
        docker push "${image_name}"
        if [ $? -eq 0 ]; then
            echo -e "${GREEN}Successfully pushed ${image_name}${NC}"
        else
            echo -e "${RED}Failed to push ${image_name}${NC}"
            return 1
        fi
    fi

    echo ""
}

# Build all images
echo -e "${BLUE}Building all images with tag: ${TAG}${NC}"
if [ -n "$REGISTRY" ]; then
    echo -e "Registry: ${REGISTRY}"
fi
echo ""

# Build mysticentral (backend)
build_image "mysticentral" "${PROJECT_ROOT}/mysticentral"

# Build frontend
build_image "mystiproxy-frontend" "${PROJECT_ROOT}/frontend"

# Summary
echo -e "${GREEN}========================================"
echo "  Build Complete!"
echo "========================================${NC}"
echo ""
echo "Images built:"
if [ -n "$REGISTRY" ]; then
    echo "  - ${REGISTRY}/mysticentral:${TAG}"
    echo "  - ${REGISTRY}/mystiproxy-frontend:${TAG}"
else
    echo "  - mysticentral:${TAG}"
    echo "  - mystiproxy-frontend:${TAG}"
fi
echo ""

# Show image sizes
echo -e "${BLUE}Image sizes:${NC}"
docker images --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}" | grep -E "REPOSITORY|mysticentral|mystiproxy-frontend" | grep "${TAG}"
