# 基础设施架构设计

**日期**: 2024-04-04  
**优先级**: P0  
**状态**: 待审批

---

## 1. 概述

### 1.1 目标

构建**生产级基础设施栈**，支持微服务架构和完整可观测性：
- **OpenTelemetry**：统一日志、指标、分布式追踪
- **gRPC 微服务**：策略/执行/数据/风控服务拆分
- **优雅关闭**：信号处理、资源清理、状态持久化
- **Docker 部署**：容器化开发、本地测试、Kubernetes 生产部署
- **热加载配置**：运行时配置更新，无需重启
- **API 网关**：统一入口、限流、熔断、审计

### 1.2 设计原则

- **可观测性优先**：所有组件可监控、可追踪、可告警
- **零停机升级**：配置热加载、滚动更新、零数据丢失
- **强类型通信**：Protobuf 定义，接口契约明确
- **渐进式部署**：本地 docker-compose -> Kubernetes

---

## 2. 核心架构

### 2.1 微服务架构

```
┌─────────────────────────────────────────────────────────────┐
│                    API Gateway (REST/gRPC)                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Auth     │  │ Rate     │  │ Circuit  │  │ Audit      │  │
│  │ Filter   │  │ Limiter  │  │ Breaker  │  │ Logger     │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
         v                   v                   v
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Strategy     │    │ Execution    │    │ Data         │
│ Service      │    │ Service      │    │ Service      │
│ (gRPC)       │    │ (gRPC)       │    │ (gRPC)       │
└──────────────┘    └──────────────┘    └──────────────┘
         │                   │                   │
         v                   v                   v
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Risk         │    │ Position     │    │ Cache        │
│ Service      │    │ Manager     │    │ Manager      │
│ (gRPC)       │    │ (gRPC)      │    │ (Redis)      │
└──────────────┘    └──────────────┘    └──────────────┘
```

### 2.2 服务通信

```
┌─────────────────────────────────────────────────────────────┐
│                    OpenTelemetry Collector                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Metrics  │  │ Logs     │  │ Traces   │  │ Export     │  │
│  │ (Prom)   │  │ (ELK)    │  │ (Jaeger) │  │ (S3)       │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
         ▲                   ▲                   ▲
         │                   │                   │
         v                   v                   v
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Strategy     │    │ Execution    │    │ Data         │
│ Service      │    │ Service      │    │ Service      │
│ (OTel SDK)   │    │ (OTel SDK)   │    │ (OTel SDK)   │
└──────────────┘    └──────────────┘    └──────────────┘
```

---

## 3. 详细设计

### 3.1 OpenTelemetry 监控栈

#### 3.1.1 指标收集

```rust
// 核心指标
pub struct Metrics {
    // 系统指标
    pub requests_total: Counter {
        labels: ["method", "endpoint", "status_code"],
    },
    pub request_duration: Histogram {
        labels: ["endpoint", "service"],
        buckets: [0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0],
    },
    
    // 业务指标
    pub orders_total: Counter {
        labels: ["status", "venue", "strategy_id"],
    },
    pub pnl_gauge: Gauge {
        labels: ["venue", "side"],
    },
    pub risk_exposure: Gauge {
        labels: ["risk_type", "instrument"],
    },
    
    // 错误指标
    pub errors_total: Counter {
        labels: ["error_type", "service", "severity"],
    },
}

// 自动资源检测
pub struct ResourceDetector {
    pub process: ProcessResource,
    pub host: HostResource,
    pub container: ContainerResource,
    pub kubernetes: KubernetesResource,
}
```

#### 3.1.2 分布式追踪

```rust
// 追踪上下文
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub attributes: HashMap<String, Value>,
}

// 活动管理器
pub struct ActiveSpanManager {
    pub current_span: Option<ActiveSpan>,
    pub span_stack: Vec<ActiveSpan>,
}

impl ActiveSpanManager {
    pub fn enter_span(&mut self, operation: &str) -> SpanBuilder;
    
    pub fn exit_span(&mut self, duration: Duration);
    
    pub fn get_trace_id(&self) -> &str;
}
```

#### 3.1.3 日志

```rust
// OpenTelemetry 日志
pub struct OtelLogger {
    pub logger: Logger,
    pub resource: Resource,
}

impl OtelLogger {
    pub fn info(&self, context: &TraceContext, message: &str, fields: &HashMap<String, Value>);
    
    pub fn error(&self, context: &TraceContext, message: &str, fields: &HashMap<String, Value>);
}

// 结构化日志格式
pub struct StructuredLog {
    pub trace_id: String,
    pub span_id: String,
    pub severity: Severity,
    pub body: String,
    pub attributes: HashMap<String, Value>,
}
```

### 3.2 gRPC 微服务

#### 3.2.1 Protobuf 定义

```protobuf
// strategy.proto
syntax = "proto3";

package quantd.strategy;

message EvaluateRequest {
  string instrument_id = 1;
  NormalizedBar bar = 2;
  StrategyContext context = 3;
}

message EvaluateResponse {
  Signal signal = 1;
  string strategy_id = 2;
  float confidence = 3;  // 决策置信度
}

message StrategyHealth {
  string service_name = 1;
  string version = 2;
  string status = 3;     // healthy, degraded, unhealthy
  int64_uptime_ms = 4;
}

// execution.proto
syntax = "proto3";

package quantd.execution;

message ExecuteOrder {
  OrderRequest order = 1;
  string idempotency_key = 2;
}

message ExecuteResponse {
  OrderStatus status = 1;
  string order_id = 2;
  string exchange_order_id = 3;
  Fill fill = 4;  // 如果是立即成交
}

message PositionQuery {
  string instrument_id = 1;
}

message PositionResponse {
  Position position = 1;
  float unrealized_pnl = 2;
  float realized_pnl = 3;
}

// data.proto
syntax = "proto3";

package quantd.data;

message GetData {
  string instrument_id = 1;
  DataType data_type = 2;
  int64 start_ts = 3;
  int64 end_ts = 4;
}

message DataResponse {
  repeated DataItem items = 1;
  int64 total_count = 2;
}

// risk.proto
syntax = "proto3";

package quantd.risk;

message CheckOrder {
  OrderRequest order = 1;
  PortfolioState portfolio = 2;
}

message CheckResponse {
  bool allowed = 1;
  RiskScore risk_score = 2;
  string reason = 3;
  float adjusted_limit = 4;
}
```

#### 3.2.2 服务实现

```rust
// 策略服务
pub struct StrategyService {
    pub strategies: Arc<RwLock<HashMap<String, Arc<dyn Strategy>>>>,
    pub metrics: Metrics,
    pub logger: OtelLogger,
}

impl StrategyService {
    pub async fn evaluate(&self, request: EvaluateRequest) -> EvaluateResponse {
        // 1. 获取策略
        // 2. 执行评估
        // 3. 记录追踪和指标
    }
}

// 执行服务
pub struct ExecutionService {
    pub adapters: Arc<RwLock<HashMap<String, Arc<dyn ExecutionAdapter>>>>,
    pub order_manager: Arc<OrderManager>,
    pub position_manager: Arc<PositionManager>,
    pub metrics: Metrics,
    pub logger: OtelLogger,
}

impl ExecutionService {
    pub async fn execute_order(&self, request: ExecuteOrder) -> ExecuteResponse {
        // 1. 幂等检查
        // 2. 仓位检查
        // 3. 执行
        // 4. 异步更新状态
    }
    
    pub async fn query_position(&self, request: PositionQuery) -> PositionResponse {
        self.position_manager.get_position(&request.instrument_id)
    }
}
```

#### 3.2.3 服务发现

```rust
// 本地开发：Docker Compose 服务发现
pub struct LocalDiscovery {
    pub services: HashMap<String, ServiceInfo>,
}

impl LocalDiscovery {
    pub fn resolve(&self, service: &str) -> Result<SocketAddr, DiscoveryError>;
    
    pub fn health_check(&self, service: &str) -> bool;
}

// 生产：Kubernetes Service + DNS
pub struct K8sDiscovery {
    pub client: K8sClient,
    pub namespace: String,
}

impl K8sDiscovery {
    pub fn resolve(&self, service: &str) -> Result<SocketAddr, DiscoveryError>;
    
    pub fn watch(&self) -> WatchStream<ServiceEvent>;
}
```

### 3.3 优雅关闭和信号处理

#### 3.3.1 信号处理

```rust
pub struct GracefulShutdown {
    pub signal: Arc<Signal>,
    pub timeout: Duration,
    pub cleanup_hooks: Vec<Box<dyn Fn() + Send + Sync>>,
    pub state: Arc<AtomicUsize>,  // 0=running, 1=shutting_down, 2=stopped
}

impl GracefulShutdown {
    pub fn new() -> Self {
        Self {
            signal: Arc::new(Signal::new(SignalKind::all())),
            timeout: Duration::from_secs(30),
            cleanup_hooks: Vec::new(),
            state: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    pub fn register_cleanup(&mut self, hook: Box<dyn Fn() + Send + Sync>);
    
    pub fn shutdown(&self) -> Result<(), ShutdownError>;
    
    pub fn is_shutdown(&self) -> bool;
}

pub enum ShutdownError {
    Timeout,  // 超时未关闭
    Interrupt,  // 强制中断
}
```

#### 3.3.2 状态持久化

```rust
pub struct StateSaver {
    pub db: Arc<dyn Database>,
    pub checkpoint_interval: Duration,
}

impl StateSaver {
    pub async fn save_state(&self) -> Result<(), SaveError>;
    
    pub async fn restore_state(&self) -> Result<(), RestoreError>;
    
    pub async fn checkpoint(&self) -> Result<(), CheckpointError>;
}
```

#### 3.3.3 资源清理

```rust
pub struct ResourceLeakDetector {
    pub memory_usage: Arc<AtomicU64>,
    pub threshold_mb: usize,
    pub check_interval: Duration,
}

impl ResourceLeakDetector {
    pub fn check(&self) -> Result<(), LeakError>;
    
    pub async fn monitor(&self) -> Result<(), MonitorError>;
}
```

### 3.4 热加载配置

#### 3.4.1 配置监听

```rust
pub struct HotReloadConfig {
    pub path: PathBuf,
    pub format: ConfigFormat,  // YAML, JSON, TOML
    pub watch_interval: Duration,
    pub validator: Box<dyn ConfigValidator>,
}

impl HotReloadConfig {
    pub fn new(path: PathBuf) -> Result<Self, ConfigError>;
    
    pub fn watch(&self) -> Result<WatchEventStream, WatchError>;
    
    pub fn reload(&self) -> Result<Config, ConfigError>;
}

pub enum WatchEvent {
    Modified { config: Config },
    Deleted,
    Error { error: ConfigError },
}
```

#### 3.4.2 配置版本控制

```rust
pub struct ConfigVersion {
    pub version: String,      // UUID
    pub hash: String,         // 内容 hash
    pub timestamp: i64,
    pub source: String,       // file, api, env
}

pub struct ConfigHistory {
    pub current: ConfigVersion,
    pub history: Vec<ConfigVersion>,
}

impl ConfigHistory {
    pub fn rollback(&mut self, version: &str) -> Result<(), RollbackError>;
    
    pub fn get_diff(&self, version1: &str, version2: &str) -> ConfigDiff;
}
```

### 3.5 API 网关

#### 3.5.1 限流和熔断

```rust
pub struct RateLimiter {
    pub tokens: TokenBucket,
    pub burst_size: usize,
    pub refill_rate: f64,  // 每秒令牌数
}

impl RateLimiter {
    pub fn acquire(&mut self) -> Result<(), RateLimitError>;
    
    pub fn check(&self, cost: usize) -> bool;
}

pub struct CircuitBreaker {
    pub state: CircuitState,  // closed, open, half_open
    pub failure_threshold: usize,
    pub success_threshold: usize,
    pub timeout: Duration,
}

impl CircuitBreaker {
    pub fn allow_request(&mut self) -> bool;
    
    pub fn record_success(&mut self);
    
    pub fn record_failure(&mut self);
}

pub enum CircuitState {
    Closed,    // 正常
    Open,      // 熔断
    HalfOpen,  // 测试
}
```

#### 3.5.2 审计日志

```rust
pub struct AuditLogger {
    pub logger: OtelLogger,
    pub db: Arc<dyn Database>,
}

impl AuditLogger {
    pub fn log_request(&self, request: &Request, response: &Response, duration: Duration);
    
    pub fn log_auth(&self, user: &str, action: &str, success: bool, details: &HashMap<String, Value>);
    
    pub fn log_security(&self, event: &SecurityEvent);
}

pub enum SecurityEvent {
    UnauthorizedAccess,
    RateLimitExceeded,
    ConfigurationChanged,
    SuspiciousActivity,
}
```

### 3.6 Docker 和 Kubernetes

#### 3.6.1 Dockerfile

```dockerfile
# 多阶段构建
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/quantd .
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
CMD ["./quantd"]
```

#### 3.6.2 Kubernetes 部署

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: quantd-strategy
spec:
  replicas: 3
  selector:
    matchLabels:
      app: quantd-strategy
  template:
    metadata:
      labels:
        app: quantd-strategy
    spec:
      containers:
      - name: quantd-strategy
        image: quantd/strategy:latest
        ports:
        - containerPort: 8080
        env:
        - name: QUANTD_ENV
          value: "production"
        - name: QUANTD_OTEL_EXPORTER
          value: "grpc"
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 15
          periodSeconds: 20
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - weight: 100
            podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: app
                  operator: In
                  values:
                  - quantd-strategy
              topologyKey: kubernetes.io/hostname
---
apiVersion: v1
kind: Service
metadata:
  name: quantd-strategy
spec:
  selector:
    app: quantd-strategy
  ports:
  - port: 8080
    targetPort: 8080
  type: ClusterIP
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: quantd-strategy
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: quantd-strategy
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

---

## 4. 数据模型

### 4.1 核心类型

```protobuf
// common.proto
message Timestamp {
  int64 seconds = 1;
  int32 nanos = 2;
}

message Money {
  string currency = 1;  // USD, CNY
  string amount = 2;    // 避免浮点数精度问题
}

message Instrument {
  string symbol = 1;
  string venue = 2;
  string base_currency = 3;
  string quote_currency = 4;
}

// metrics.proto
message MetricPoint {
  string metric_name = 1;
  string tags = 2;      // JSON 格式标签
  double value = 3;
  int64 timestamp = 4;
}
```

---

## 5. 执行流程

### 5.1 服务启动流程

```
1. 加载配置（支持热加载）
2. 初始化 OpenTelemetry（指标/日志/追踪）
3. 注册信号处理器（SIGTERM, SIGINT）
4. 启动本地服务发现（Docker Compose）
5. 初始化数据库连接
6. 启动 gRPC 服务器
7. 启动 HTTP 健康检查
8. 等待就绪
```

### 5.2 服务关闭流程

```
1. 收到信号 -> 设置 shutdown 标志
2. 优雅关闭定时器（30 秒）
3. 取消所有异步任务
4. 保存状态到数据库
5. 清理资源（数据库连接、文件句柄）
6. 退出程序
```

### 5.3 配置热加载流程

```
1. 监听配置文件变化
2. 解析新配置
3. 验证配置（schema 校验）
4. 生成新版本号
5. 应用新配置（原子更新）
6. 记录审计日志
7. 通知依赖服务
```

---

## 6. 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum InfraError {
    #[error("OTel error: {0}")]
    Otel(String),
    
    #[error("gRPC error: {0}")]
    Grpc(String),
    
    #[error("Shutdown error: {0}")]
    Shutdown(String),
    
    #[error("Resource error: {0}")]
    Resource(String),
    
    #[error("Discovery error: {0}")]
    Discovery(String),
}
```

---

## 7. 配置管理

```yaml
# infra_config.yaml
otel:
  exporter: "grpc"
  grpc_endpoint: "collector:4317"
  sampling_rate: 1.0
  
gRPC:
  server_address: "0.0.0.0:50051"
  max_message_size: 104857600  # 100MB
  
shutdown:
  timeout_seconds: 30
  save_state: true
  
hot_reload:
  enabled: true
  path: "/etc/quantd/config.yaml"
  interval_seconds: 5
  
docker:
  health_check:
    path: /health
    interval: 30
    timeout: 10
    retries: 3
```

---

## 8. 测试策略

### 8.1 单元测试

```rust
#[test]
fn test_graceful_shutdown() {
    let shutdown = GracefulShutdown::new();
    shutdown.register_cleanup(|| {
        println!("Cleanup hook called");
    });
    assert!(!shutdown.is_shutdown());
}

#[test]
fn test_circuit_breaker() {
    let mut breaker = CircuitBreaker::new(3, 2, Duration::from_secs(5));
    assert!(breaker.allow_request());  // 正常
    for _ in 0..3 {
        breaker.record_failure();
    }
    assert!(!breaker.allow_request());  // 熔断
}
```

### 8.2 集成测试

```rust
#[tokio::test]
async fn test_grpc_service_health() {
    let service = StrategyService::new();
    let response = service.health().await;
    assert_eq!(response.status, "healthy");
}
```

---

## 9. 使用示例

```rust
// 1. 初始化服务
let shutdown = GracefulShutdown::new();
let otel = OtelLogger::new();

// 2. 启动 gRPC 服务
let service = StrategyService::new(otel.clone());
let server = tonic::transport::Server::builder()
    .add_service(strategy::StrategyServiceServer::new(service))
    .serve_with_shutdown("[::]:50051".parse().unwrap(), async { shutdown.shutdown() });

// 3. 优雅关闭
shutdown.shutdown().await?;
server.await??;
```

---

## 10. 实施计划

### 阶段 1：核心框架（2 周）
- [ ] OpenTelemetry 集成（指标/日志/追踪）
- [ ] gRPC 服务定义和实现
- [ ] 优雅关闭和信号处理

### 阶段 2：微服务架构（2 周）
- [ ] 策略服务、执行服务拆分
- [ ] Docker Compose 本地开发
- [ ] 服务发现和负载均衡

### 阶段 3：生产部署（2 周）
- [ ] Kubernetes 部署文件
- [ ] HPA 自动扩缩容
- [ ] 配置热加载和版本控制

---

## 11. 依赖

```toml
[dependencies]
# OpenTelemetry
opentelemetry = "0.21"
opentelemetry-otlp = "0.14"
tracing-opentelemetry = "0.22"

# gRPC
tonic = "0.10"
prost = "0.12"
prost-build = "0.12"

# 信号处理
ctrlc = "3.4"

# 配置管理
notify = "6.0"
toml = "0.8"
yaml-rust2 = "0.8"

# 容器化
docker-compose = "0.21"
kubernetes = "0.27"

# 监控
prometheus = "0.13"
```

---

**审批问题**：
1. OpenTelemetry + gRPC 架构是否符合预期？
2. 服务拆分（策略/执行/数据/风控）是否合理？
3. 实施优先级和范围是否需要调整？

请确认是否批准此设计。