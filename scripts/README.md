# Scripts

脚本按用途分组：

- `binance/`: Binance Spot Testnet paper、smoke、soak 与脚本测试。
- `ibkr/`: IBKR paper、Gateway、filled-order evidence、soak 与脚本测试。
- `check/`: 架构边界、schema、clippy、完整 verify、readiness 与脚本校验。
- `reconciliation/`: 跨 broker reconciliation gate、evidence aggregation 与 production soak。
- `smoke/`: MVP、V1、paper、ops、REST 与 server 通用冒烟脚本。

常用入口：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\smoke\v1-smoke.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\check\paper-readiness.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\binance\binance-paper-script-tests.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\ibkr\ibkr-paper-script-tests.ps1
```

生成数据按职责写入：

- `data/backtest/`: 策略回测与 feature-gate 数据库。
- `data/binance/`: Binance live snapshot、paper runs、soak evidence 与固定 paper 数据库。
- `data/ibkr/`: IBKR paper、filled-order evidence、测试 fixture 与固定 paper 数据库。
- `data/reconciliation/`: live gate replay 与 production reconciliation evidence。
- `data/verification/`: readiness、live recovery、operator evidence 与 smoke 数据库。
- `data/runtime/`: live worker launch、heartbeat 与进程状态。
- `data/trader.sqlite`: 正常业务数据库，也是 `data/` 根目录唯一保留的数据库。
