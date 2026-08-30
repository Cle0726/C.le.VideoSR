<div align="center">

# C.le.VideoSR

### 本地优先的桌面级 AI 视频超分辨率、插帧与画质修复工作站
**Local-first AI video enhancement & restoration workstation**

[![Rust](https://img.shields.io/badge/Core-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri 2](https://img.shields.io/badge/Desktop-Tauri%202-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Vulkan](https://img.shields.io/badge/GPU-Vulkan%20%7C%20NCNN-red?logo=vulkan)](https://github.com/Tencent/ncnn)
[![FFmpeg](https://img.shields.io/badge/Media-FFmpeg-green?logo=ffmpeg)](https://ffmpeg.org/)

</div>

---

## 💡 产品原则与核心承诺 (Product Policy)

`C.le.VideoSR` 致力于为创作者提供纯净、高效的本地视频画质修复环境：

- 🆓 **核心功能永久免费**：本地视频超分辨率、插帧与画质修复完全免费。
- ♾️ **无时长与次数限制**：本地推理无计费墙、无额度限制、无水印。
- 🔒 **纯本地隐私安全**：无需上传云端 API，保障视频与创作素材绝对安全。
- 🛡️ **工业级受限流式调度**：采用「受限解码 → AI 帧处理 → 硬件编码」流水线，杜绝长视频爆显存 (OOM)。

---

## ✨ 核心特性

- 🚀 **高性能解耦架构**：Desktop Shell（Tauri 2 + React）、任务调度内核（Rust）与推理引擎彻底解耦，易于扩展。
- 🎛️ **三大核心处理模式**：
  - **Fast（快速增强）**：轻度锐化与插帧，适合低算力硬件或快速预览。
  - **Quality（画质优先）**：平衡伪影抑制与细节重建，适合高清重制。
  - **AI Restore（深度修复）**：消除老旧胶片噪点与压缩伪影，重塑纹理细节。
- 🔌 **可插拔推理后端**：首选搭载高性能 NCNN/Vulkan 引擎，规划支持 TensorRT 与自定义 Python Worker。
- 🔄 **完善的任务控制**：支持本机文件检测 (`ffprobe`)、实时进度反馈、结构化错误报告与随时可取消的任务队列。

---

## 🛠️ 当前里程碑 (Current Milestone: M3)

- [x] Tauri 2 + React + TypeScript 桌面工作台与 C.le. 深色视觉体系
- [x] 原生文件选择与 `ffprobe` 媒体信息深度探查
- [x] Rust 控制的 FFmpeg 与 AI 任务队列（支持实时进度与中断）
- [x] H.264 / H.265 编码输出与 M1 流拷贝验证链路
- [x] 带版本控制的 Runtime & Model Manifest 清单与 CI 校验

---

## 📄 许可证说明

本项目桌面端核心体验坚持免费原则。开源许可证将在第三方依赖/模型协议全面审查后正式公布。
