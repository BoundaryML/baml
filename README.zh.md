<div align="center">
<a href="https://boundaryml.com?utm_source=github" target="_blank" rel="noopener noreferrer">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="fern/assets/baml-logo-white.png">
    <img src="fern/assets/baml-logo-white.png" height="220" id="top" alt="BAML logo">
  </picture>
</a>

# BAML: Basically A Made-up Language

BAML 是专为智能体（Agents）打造的编程语言。

[![BAML Version](https://img.shields.io/pypi/v/baml-py?color=006dad&label=BAML%20Version)](https://pypi.org/project/baml-py/)

[官网首页](https://www.boundaryml.com/) | [探索 BAML](https://www.boundaryml.com/explore) | [Discord 社区](https://www.boundaryml.com/discord)

<p align="center">
  <a href="README.md">English</a> · <b>简体中文</b>
</p>

</div>

BAML 的语法看起来像 TypeScript，但它的每一项特性都是为了让智能体少犯错误而专门构建的：

- 拥有如 Rust 般严谨的类型系统，但编译速度比 Go 还要快。
- 类型在运行时持续生效。不存在 `any` 类型，也不允许危险地强制转换为任意类型。
- 错误具备强类型定义并支持静态分析。
- 文件系统结构直接映射为模块与命名空间（Modules/Namespaces）。
- 支持绿色线程（Green Threads），并具备如 Go 语言般无传染性的并发（Colorless Concurrency）模型。
- 内置测试与评估（Eval）框架。
- 内置面向智能体的标准库（stdlib）。
- 每一个 BAML 工具均原生为智能体设计，杜绝冗余脏输出等问题。
- 既可独立运行，也支持渐进式集成（您可以从 TypeScript、Python、Go、C#、Java 等语言中直接调用 BAML 函数）。

[浏览官方网站与示例](https://www.boundaryml.com/explore)。

## 快速体验

```bash
brew install baml
baml agent install
baml init
baml ide install --code
```

或者阅读[快速入门指南](https://boundaryml.com/quickstart)。

## 参与贡献

请参阅我们的[入门贡献指南](/CONTRIBUTING.md)。

---

由 Boundary 用 ❤️ 打造。总部位于美国华盛顿州西雅图。

我们正在招聘热爱 Rust 的软件工程师。欢迎[发送邮件联系我们](mailto:founders@boundaryml.com)或在 [Discord](https://www.boundaryml.com/discord) 上与我们交流。

---

> **翻译说明（Translation Disclaimer）**
> 本文档由社区志愿者（[@JasonYeYuhe](https://github.com/JasonYeYuhe)）人工翻译并维护，旨在降低中文开发者的阅读门槛。最后同步日期：2026年10月26日。若译文与英文原版存在任何出入或滞后，请以 [英文原版 (README.md)](README.md) 为准。欢迎通过 PR 协助改进译文！
