#!/bin/bash
# Deploy MystiProxy Mock Management System to Kubernetes
# Usage: ./scripts/deploy-k8s.sh [options]
#
# Options:
#   -n, --namespace    Kubernetes namespace (default: mock-management)
#   -d, --dry-run      Show what would be deployed without applying
#   -h, --help         Show this help message

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
NAMESPACE="mock-management"
DRY_RUN=false
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
K8S_DIR="${PROJECT_ROOT}/k8s"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        -d|--dry-run)
            DRY_RUN=true
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
echo "  MystiProxy Kubernetes Deployer"
echo "========================================"
echo -e "${NC}"

# Check kubectl
if ! command -v kubectl &> /dev/null; then
    echo -e "${RED}Error: kubectl is not installed${NC}"
    exit 1
fi

# Check cluster connection
echo -e "${YELLOW}Checking Kubernetes cluster connection...${NC}"
if ! kubectl cluster-info &> /dev/null; then
    echo -e "${RED}Error: Cannot connect to Kubernetes cluster${NC}"
    exit 1
fi
echo -e "${GREEN}Connected to Kubernetes cluster${NC}"
echo ""

# Deploy function
deploy_resources() {
    local file=$1
    local name=$(basename "$file")

    echo -e "${YELLOW}Deploying ${name}...${NC}"

    if [ "$DRY_RUN" = true ]; then
        kubectl apply -f "$file" --dry-run=client -o yaml
        echo -e "${BLUE}(Dry run - no changes made)${NC}"
    else
        kubectl apply -f "$file"
    fi

    echo ""
}

# Deployment order
echo -e "${BLUE}Deploying to namespace: ${NAMESPACE}${NC}"
if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}(Dry run mode - no changes will be made)${NC}"
fi
echo ""

# 1. Create namespace first
echo -e "${YELLOW}=== Step 1: Creating Namespace ===${NC}"
deploy_resources "${K8S_DIR}/namespace.yaml"

if [ "$DRY_RUN" = false ]; then
    # Wait for namespace to be ready
    kubectl get namespace "$NAMESPACE" &> /dev/null || {
        echo -e "${RED}Failed to create namespace${NC}"
        exit 1
    }
fi

# 2. Deploy PostgreSQL
echo -e "${YELLOW}=== Step 2: Deploying PostgreSQL ===${NC}"
deploy_resources "${K8S_DIR}/postgres.yaml"

if [ "$DRY_RUN" = false ]; then
    echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
    kubectl rollout status statefulset/postgres -n "$NAMESPACE" --timeout=120s
fi

# 3. Deploy MystiCentral (backend)
echo -e "${YELLOW}=== Step 3: Deploying MystiCentral ===${NC}"
deploy_resources "${K8S_DIR}/mysticentral.yaml"

if [ "$DRY_RUN" = false ]; then
    echo -e "${YELLOW}Waiting for MystiCentral to be ready...${NC}"
    kubectl rollout status deployment/mysticentral -n "$NAMESPACE" --timeout=120s
fi

# 4. Deploy Frontend
echo -e "${YELLOW}=== Step 4: Deploying Frontend ===${NC}"
deploy_resources "${K8S_DIR}/frontend.yaml"

if [ "$DRY_RUN" = false ]; then
    echo -e "${YELLOW}Waiting for Frontend to be ready...${NC}"
    kubectl rollout status deployment/frontend -n "$NAMESPACE" --timeout=120s
fi

# Summary
echo -e "${GREEN}========================================"
echo "  Deployment Complete!"
echo "========================================${NC}"
echo ""

if [ "$DRY_RUN" = false ]; then
    # Show deployment status
    echo -e "${BLUE}Deployment Status:${NC}"
    kubectl get all -n "$NAMESPACE"
    echo ""

    # Show ingress
    echo -e "${BLUE}Ingress:${NC}"
    kubectl get ingress -n "$NAMESPACE"
    echo ""

    # Show access information
    echo -e "${BLUE}Access Information:${NC}"
    echo "  Frontend: http://mystiproxy.local"
    echo "  Backend API: http://api.mystiproxy.local"
    echo ""
    echo -e "${YELLOW}Note: Add the following entries to your /etc/hosts:${NC}"
    echo "  <INGRESS_IP> mystiproxy.local api.mystiproxy.local"
    echo ""
    echo -e "${YELLOW}To get the Ingress IP, run:${NC}"
    echo "  kubectl get ingress -n ${NAMESPACE} -o wide"
else
    echo -e "${YELLOW}Dry run completed. No changes were made.${NC}"
fi
