#!/bin/bash
# Deploy MystiProxy Mock Management System to Kubernetes
# 使用 kubectl 直接部署，不依赖 Helm

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

NAMESPACE="mock-management"
K8S_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../k8s" && pwd)"

echo -e "${BLUE}"
echo "========================================"
echo "  MystiProxy Kubernetes Deployer"
echo "  (使用 MystiProxy 自身静态服务能力)"
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

# 1. Create namespace
echo -e "${YELLOW}=== Step 1: Creating Namespace ===${NC}"
kubectl apply -f "${K8S_DIR}/namespace.yaml"

# 2. Deploy PostgreSQL
echo -e "${YELLOW}=== Step 2: Deploying PostgreSQL ===${NC}"
kubectl apply -f "${K8S_DIR}/postgres.yaml"
echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
kubectl rollout status statefulset/postgres -n "$NAMESPACE" --timeout=120s || true

# 3. Deploy MystiCentral
echo -e "${YELLOW}=== Step 3: Deploying MystiCentral ===${NC}"
kubectl apply -f "${K8S_DIR}/mysticentral.yaml"
echo -e "${YELLOW}Waiting for MystiCentral to be ready...${NC}"
kubectl rollout status deployment/mysticentral -n "$NAMESPACE" --timeout=120s || true

# 4. Create frontend static files ConfigMap
echo -e "${YELLOW}=== Step 4: Creating Frontend Static Files ===${NC}"
if [ -d "${K8S_DIR}/../frontend/dist" ]; then
    kubectl create configmap frontend-static-files \
        --from-file="${K8S_DIR}/../frontend/dist" \
        -n "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -
else
    echo -e "${YELLOW}Frontend dist not found, creating placeholder...${NC}"
    kubectl create configmap frontend-static-files \
        --from-literal=index.html='<html><body><h1>MystiProxy Mock Management</h1></body></html>' \
        -n "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -
fi

# 5. Deploy Frontend (MystiProxy as static server)
echo -e "${YELLOW}=== Step 5: Deploying Frontend (MystiProxy) ===${NC}"
kubectl apply -f "${K8S_DIR}/frontend.yaml"
echo -e "${YELLOW}Waiting for Frontend to be ready...${NC}"
kubectl rollout status deployment/frontend -n "$NAMESPACE" --timeout=120s || true

# Summary
echo -e "${GREEN}========================================"
echo "  Deployment Complete!"
echo "========================================${NC}"
echo ""

echo -e "${BLUE}Deployment Status:${NC}"
kubectl get all -n "$NAMESPACE"
echo ""

echo -e "${BLUE}Access Information:${NC}"
echo "  Frontend: http://mystiproxy.local"
echo "  Backend API: http://api.mystiproxy.local"
echo ""
echo -e "${YELLOW}Note: Add the following entries to your /etc/hosts:${NC}"
echo "  127.0.0.1 mystiproxy.local api.mystiproxy.local"
echo ""

# Port forward for local testing
echo -e "${YELLOW}To access locally, run:${NC}"
echo "  kubectl port-forward -n ${NAMESPACE} svc/frontend-service 8080:80 &"
echo "  kubectl port-forward -n ${NAMESPACE} svc/mysticentral-service 8081:8080 &"
