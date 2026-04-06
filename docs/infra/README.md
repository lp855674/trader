# Infrastructure System Documentation

## Architecture

The `infra` crate provides cross-cutting infrastructure for all platform components.

### Modules

| Module | Purpose |
|--------|---------|
| `otel/` | In-process metrics, distributed tracing, structured logging |
| `lifecycle/` | Signal handling, graceful shutdown, resource cleanup, watchdog, state saver |
| `services/` | Service stubs for strategy, execution, data, risk; service discovery, circuit breaker, load balancer |
| `grpc/` | In-process gRPC server stub, health check service |
| `config/` | Config loader, validator, hot reload, versioning, schema, encryption, audit |

## Deployment Guide

### Docker

```bash
# Build
docker build -t trader:latest .

# Run locally
docker-compose up -d

# Check health
curl http://localhost:8080/health
```

### Kubernetes

```bash
# Create namespace
kubectl create namespace trading

# Apply manifests
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/hpa.yaml
kubectl apply -f k8s/service.yaml

# Check pods
kubectl get pods -n trading

# View logs
kubectl logs -n trading -l app=trader -f
```

### GitOps (Flux)

```bash
# Bootstrap Flux
flux bootstrap github \
  --owner=your-org \
  --repository=trader \
  --branch=main \
  --path=k8s

# Check reconciliation
flux get kustomizations
```

## Operational Procedures

### Graceful Shutdown
The platform uses a 4-phase shutdown sequence:
1. **Initiated** — stop accepting new orders
2. **DrainConnections** — complete in-flight requests
3. **SaveState** — checkpoint all state to disk
4. **Complete** — process exits

### Circuit Breaker
Threshold-based circuit breaker protects downstream services:
- Opens after `failure_threshold` consecutive failures
- Transitions to half-open after reset timeout
- Closes after `half_open_threshold` consecutive successes

### Watchdog
All long-running services register heartbeats. Services missing heartbeats beyond their `timeout_ms` are flagged as unhealthy.

## Security Policies

- All secrets encrypted at rest via `SecretEncryptor` (XOR-based stub; replace with KMS in production)
- Config changes are fully audited via `AuditLogger`
- mTLS enforced between services via Istio `PeerAuthentication`
- Non-root container user (UID 10001)
- Kubernetes `PodDisruptionBudget` ensures minimum availability

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Pod crash loop | Bad config | Check `validate_config()` output |
| Circuit open | Downstream unavailable | Check dependent service health |
| Slow startup | State restore | Reduce snapshot size |
| Alert storm | Low thresholds | Tune watchdog `timeout_ms` |
| HPA not scaling | Metrics not emitting | Check `MetricsCollector` |
