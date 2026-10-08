# 多平台桌面构建与验证基础设施（Windows / Linux）— ticket #35

日期：2026-10-08。范围：本机（Apple Silicon mac）搭建 Windows 与 Linux 桌面端**真实**构建与验证环境；Linux 完成「源码 → 原生解码器 → cargo 构建 → deb/AppImage 打包 → dpkg 真实安装 → HEIC 烟测」端到端实证；Windows 完成路径评估与阻塞记录。分支 `codex/release-infra`。

## 1. 结论速览

| 平台 | 结论 | 实证 |
|---|---|---|
| Linux arm64 | **端到端打通**。VM 内原生构建 deb+AppImage，真实安装并烟测通过 | §5，验证器 JSON 全 pass |
| Linux x86_64 | 本地 TCG 全模拟**只可用于启动级验证，不可用于构建**（实测 15–27× 差距）；x86_64 产物继续走 GitHub Actions x86_64 runner（既有 CI 证据包 `docs/verification/packaged-ubuntu-24.04/`） | §6 基准数据 |
| Windows | **VM 未建**：官方 Win11 ARM64 ISO 被微软下载流程挡住（脚本客户端 403、链接由页面 JS 会话生成）；组件清单与命令草案已备好，ISO 就位后一轮可实证 | §7 |

## 2. 环境拓扑

- 宿主：macOS 26.6.2 (arm64)，10 核，16GB。
- 虚拟化：**QEMU 11.1.2（Homebrew，已存在，无需新装）**。未安装 UTM——UTM cask 会写入系统盘 /Applications（系统盘仅 ~20GB 可用），且 QEMU headless 完全覆盖本需求；因此不存在 UTM 数据目录迁移问题（`~/Library/Containers/com.utmapp.UTM` 未产生）。
- arm64 VM：`-machine virt -accel hvf -cpu host -smp 4 -m 6144`（Hypervisor.framework 近原生），Ubuntu 24.04 Server cloud image（arm64，release 20260911.20260926，SHA256 已核）+ cloud-init NoCloud seed（hdiutil 制作 CIDATA ISO）。
- amd64 VM：`-machine q35 -accel tcg -smp 4 -m 4096`（TCG 全模拟，仅作可行性测量）。
- 网络：user-mode，hostfwd `127.0.0.1:2222`→arm64 VM:22、`2223`→amd64 VM:22。
- VM 内通过清华 mirror（ubuntu-ports/ubuntu）+ nodesource 完成 cloud-init 自动装包：build-essential、pkg-config、cmake、patchelf、libwebkit2gtk-4.1-dev、libgtk-3-dev、gnome-keyring、dbus-x11、libfuse2t64、dpkg-dev、rustup stable（rustc 1.99.0）、Node 22。

## 3. 磁盘布局（全部在备份盘，系统盘零占用）

```
/Users/links/.codex/external-workspace-volume/codex/vm/   ← 200GiB 备份分区（HFS+ 物理分区 disk5s2）
├── iso/     ubuntu-24.04-server-cloudimg-{arm64,amd64}.img  各 ~620MB（cloud-images.ubuntu.com，SHA256SUMS 校验）
├── disks/   wardrobe-build-arm64.qcow2  40G 虚拟 / 实占 ~8.5G（构建后）
│            wardrobe-build-amd64.qcow2  30G 虚拟 / 实占 ~0.7G
├── seeds/   user-data-{arm64,amd64}、meta-data、seed-*.iso、SSH key（id_ed25519_wardrobe_build）
└── logs/    {arm64,amd64}-serial.log、pid
```

- 预算修正：委派时按备份盘 159Gi 可用估，实测 df 为 **70Gi 可用**，故磁盘从 60G×2 收缩为 40G+30G（qcow2 稀疏，实际使用远低于虚拟值）。
- 镜像/缓存/构建产物全部位于备份分区；系统盘未写任何 GB 级内容（Homebrew 未使用、无 UTM、ms-playwright 仍走 native-env 重定向）。
- 系统盘 df：任务开始 `/` 20Gi 可用；结束时见 §8（差异来自并行任务与既有用户缓存，非本工作，逐项核对见 §8）。

## 4. 复现步骤（全部命令可原样重跑）

宿主侧（脚本：`scripts/vm/wardrobe-build-vm.sh`；seed 配置入库 `scripts/vm/seeds/`）：

```sh
scripts/vm/wardrobe-build-vm.sh create-arm64    # 下载/校验镜像 → 克隆磁盘 40G → seed ISO → 启动 + cloud-init 自动装依赖
scripts/vm/wardrobe-build-vm.sh ssh-arm64       # 等待 cloud-init status: done（首次 ~1-2 分钟）
rsync -a --exclude .git --exclude target --exclude node_modules \
  -e "scripts/vm 的 ssh 参数" ./ ubuntu@127.0.0.1:src/    # 源码同步（~30MB）
```

VM 侧（脚本：`scripts/vm/linux-build-and-package.sh`，与 CI `.github/workflows/media-bundles.yml` Linux leg 逐步对应）：

```sh
ssh vm 'cd ~/src && RUN_TAURI_BUILD=1 bash scripts/vm/linux-build-and-package.sh'
# 1) scripts/bootstrap-media-unix.sh ~/wardrobe-media   ← libde265 v1.1.3 + libheif v1.23.1（与 CI 同 pin）
# 2) PKG_CONFIG_PATH/LD_LIBRARY_PATH + RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib:$ORIGIN/../lib/wardrobe-agent'
#    cargo build --release -p wardrobe-cli
# 3) python3 scripts/prepare-media-distribution.py … --target aarch64-unknown-linux-gnu（portable CLI+私有解码库+tauri 覆盖配置）
# 4) python3 scripts/verify-packaged-cli.py portable CLI …（同 CI）
# 5) npm ci && npm run tauri build -- --config ~/distribution/tauri-media.json → deb + AppImage
```

真实安装与烟测（CI 只做 `dpkg --extract`；本环境升级为 `dpkg -i` 真安装，并因此发现 §5.3 缺陷）：

```sh
bash scripts/vm/repack-deb-package-name.sh 电子衣橱_0.1.0_arm64.deb /tmp/wardrobe-desktop_0.1.0_arm64.deb wardrobe-desktop
sudo dpkg -i /tmp/wardrobe-desktop_0.1.0_arm64.deb
env -i PATH=/usr/bin:/bin /usr/bin/wardrobe-cli --version
python3 scripts/verify-packaged-cli.py /usr/bin/wardrobe-cli crates/wardrobe-core/tests/fixtures/generated-colors.heic --report installed-deb-runtime.json
ldd /usr/bin/wardrobe-desktop   # libheif.so.1/libde265.so.0 均解析到 /usr/bin/../lib/wardrobe-agent/，无 not found
```

## 5. Linux arm64 端到端结果（2026-10-08 实测）

### 5.1 构建
- 原生解码器：libde265 v1.1.3 + libheif v1.23.1 源码构建于 VM（`bootstrap-media-unix.sh`，与 CI 同 pin、同开关：无 x265/GPL、无插件加载）。
- `cargo build --release -p wardrobe-cli`：**2 分 15 秒**（4 vCPU HVF，冷 target）。
- `npm ci` + vite 前端 + `tauri build`：全部成功，产出：
  - `apps/desktop/src-tauri/target/release/bundle/deb/电子衣橱_0.1.0_arm64.deb`（34,381,616 B）
  - `…/appimage/电子衣橱_0.1.0_aarch64.AppImage`
- 产物 SHA256（VM 内 sha256sum）：
  - deb（repack 后 wardrobe-desktop_0.1.0_arm64.deb）`3400b2b2…819419`
  - AppImage `d6547b5f…4ac3a7`
  - target/release/wardrobe-cli `454d7c91…a9f2`

### 5.2 安装与烟测（全部通过）
1. `dpkg -i`（repack 后包）：Unpacking → Setting up → hicolor-icon-theme 触发器，**真实系统安装**。包内容：`/usr/bin/wardrobe-cli`、`/usr/bin/wardrobe-desktop`、`/usr/lib/wardrobe-agent/lib{heif,de265}.so`、桌面入口、图标、第三方许可。
2. `ldd /usr/bin/wardrobe-desktop`：`libheif.so.1 => /usr/bin/../lib/wardrobe-agent/libheif.so.1`、`libde265.so.0 => …/libde265.so.0`，**无 not found**（RUNPATH 与 CI 一致）。
3. `env -i PATH=/usr/bin:/bin wardrobe-cli --version` → `{"result":{"name":"wardrobe-cli","version":"0.1.0"},"schema_version":1}`。
4. `verify-packaged-cli.py /usr/bin/wardrobe-cli generated-colors.heic`（开发加载环境变量全剥离）：capabilities=3、stdin 令牌授权、未授权 CLI/MCP 双双拒绝、**真实 HEIC 解码 64×32**、重启后再读通过、MCP 检索一致 → 全 pass（`installed-deb-runtime.json`）。
5. 持久库手工流程：建空间（space.default 得 member_id ULID）→ 授权 → **assets.import 真实 HEIC** → 产出 64×32 webp 预览资产 → assets.get。
6. AppImage 解包 CLI 同样通过 --version 与建空间烟测。

### 5.3 发现的缺陷：deb 包名违反 Debian policy（本轮重要产出）
- tauri-bundler 直接以 `productName`（电子衣橱）作为 `Package:` 字段。`dpkg -i` 报 `invalid package name in 'Package' field: must start with an alphanumeric character`（Debian policy 5.6.1 要求 `^[a-z0-9][a-z0-9+.-]+$`）。
- **既有 CI 的 `dpkg --extract` 烟测不校验 control 文件，因此该缺陷在 `docs/verification/packaged-ubuntu-*/` 证据包中一直未暴露**；macOS DMG 不受影响。
- 本轮以 `scripts/vm/repack-deb-package-name.sh`（`dpkg-deb -R` 改 control 后重打包）完成验证路径闭环，包名暂定 `wardrobe-desktop`。
- **正式修复需 release workstream 决策**（影响全平台安装物命名）：将 `tauri.conf.json` 的 `productName` 改为 ASCII（影响窗口标题之外的所有 bundle 文件名，窗口标题由 `app.windows[].title` 独立控制、不受影响），或升级 tauri-bundler 后确认是否支持 deb 包名覆盖。该决策不在本基础设施轮次内代做。

## 6. x86_64 本地构建可行性测量（2026-10-08 实测，同法同负载）

| 负载（单线程 `-j1`） | arm64 VM（HVF） | amd64 VM（TCG） | 差距 |
|---|---|---|---|
| `openssl speed -seconds 5 sha256`（16384B 块吞吐） | 2,960,195.58k（~2.89 GB/s） | 109,107.61k（~106 MB/s） | **27.1×** |
| cargo 新建工程 + serde_json 依赖，`cargo build --release -j1`（注册表已热、target 冷） | **2.184s** | **33.406s** | **15.3×** |

外推：arm64 VM 上整套管线（解码器构建 + CLI release 构建 2m15s + 前端 + src-tauri release + 打包）约 25 分钟；按 15× 以上折扣，TCG x86_64 本地全量构建需 **6 小时以上量级**（tract-onnx、webkit 打包等大件放大差距）。

**结论**：本地 x86_64 TCG 只可用于「安装/烟测/启动级」验证，**不可用作构建线**。x86_64 产物继续由 GitHub Actions `ubuntu-24.04`（x86_64）runner 产出（既有证据：`docs/verification/packaged-ubuntu-24.04/report.md`，GitHub run 37508783217）；本地 amd64 VM（`scripts/vm/wardrobe-build-vm.sh create-amd64`，已建成可启动）保留用于未来 x86_64 产物的真实安装烟测。

## 7. Windows 路径：阻塞与所需组件

### 7.1 ISO 获取障碍（如实记录，2026-10-08 实测）
| 尝试 | 结果 |
|---|---|
| `curl -I https://www.microsoft.com/software-download/windows11arm64` | HTTP/2 **403**（拒绝脚本客户端） |
| 浏览器 UA 抓取同页 | HTTP 200，页面存在（"Download Windows 11 Arm64"），但下载链接由页面 JS 多步会话生成，脚本拿不到直链 |
| uupdump.net | 可达（备选，需较长组包时间） |

结论：官方 ISO 无需付费/开发者注册，但**必须真实浏览器会话**完成下载（约 5.5GB → `codex/vm/iso/`）。未装 pwsh（Fido 方案需要，cask 会写系统盘，留待用户确认）。ISO 就位后：`qemu-system-aarch64 -accel hvf -cpu host` 建 Win11 ARM VM（与 arm64 Linux 同加速路径，已实证可用）→ 装 VS 2022 → 按 §7.2/§7.3 实证。

### 7.2 所需 VS 组件（源自 CI windows leg 与 `bootstrap-media-windows.ps1`，非推测）
产品目标 **x86_64-pc-windows-msvc**（libheif 走 vcpkg）：
- VS 2022（Community 可）+ 工作负载「使用 C++ 的桌面开发」：
  - `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`（MSVC v143；ARM64 宿主交叉 x64 工具集）
  - `Microsoft.VisualStudio.Component.Windows11SDK.22621`（任一近期 Win11 SDK）
  - `Microsoft.VisualStudio.Component.VC.Redist.14.Latest`（app-local CRT，CI 从 `VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT` 提取）
  - `Microsoft.VisualStudio.Component.VC.CMake.Project`（vcpkg 编 libheif/libde265 需要）
- Git、Python 3、Node 22、rustup（stable-x86_64-pc-windows-msvc）、vcpkg pin `2026.07.29`

### 7.3 构建命令草案（VM 内 PowerShell，与 CI 一一对应）
```powershell
.\scripts\bootstrap-media-windows.ps1 -MediaRoot C:\wardrobe-media   # vcpkg install 'libheif[core]:x64-windows' --recurse
cargo build --release -p wardrobe-cli
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath
$crt = Get-ChildItem "$vs\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT" -Directory | Select-Object -Last 1
python scripts\prepare-media-distribution.py "$env:VCPKG_ROOT\installed\x64-windows" target\release\wardrobe-cli.exe C:\distribution --target x86_64-pc-windows-msvc --windows-crt-dir $crt.FullName
cd apps\desktop; npm ci; npm run tauri build -- --config C:\distribution\tauri-media.json
# 产物 bundle\nsis\*.exe；安装 .\*.exe /S /D=C:\wardrobe-installed 后用 verify-packaged-cli.py 烟测
```

## 8. 系统盘占用核对
- 开始：`/`（Data 卷）可用 20Gi；结束：见提交时 `df -h /` 输出（随附于验证记录）。
- 本工作系统盘写入：仅 ~KB 级（shell 历史、known_hosts 指向 /dev/null）。核查过 `~/Library/Caches`（9.1GB，既有用户应用缓存）、`~/.cargo`(1.1G)/`~/.rustup`(581M)（宿主既有工具链，供并行 cargo 测试任务使用）、无 `com.utmapp.UTM`、ms-playwright 缓存 4KB（已被 native-env.sh 重定向）。中间差额来自并行 GUI/cargo 测试任务与 macOS 可清除空间波动。

## 9. 遗留与建议
1. Windows：浏览器手动下载 ISO → 建.Win11 ARM VM → 按草案实证（ISO 是唯一阻塞）。
2. deb 包名缺陷：由 release workstream 决策 ASCII bundle 名或上游覆盖方案；决策后 CI 应把烟测从 `dpkg --extract` 升级为 `dpkg -i`（本环境已示范）。
3. x86_64 交付线：维持 CI runner 构建 + 本地 VM 安装/烟测；若未来需要本地 x86_64 构建，建议外购/借用 x86_64 硬件或云 runner，而非 TCG。
4. 重量级验证（VM 内全量 `cargo test --workspace`、cutout 真模型验证）按约束等 #34 完成后运行，命令已在脚本中就位。
