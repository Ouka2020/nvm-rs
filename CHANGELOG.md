# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4](https://github.com/Ouka2020/nvm-rs/compare/v0.1.3...v0.1.4) - 2026-10-05

### Other

- *(deps)* 更新依赖库版本并配置Windows交叉编译环境

## [0.1.3](https://github.com/Ouka2020/nvm-rs/compare/v0.1.2...v0.1.3) - 2026-10-03

### Added

- 添加大量单元测试覆盖核心功能
- 仅支持 Windows 平台并完善目录与镜像配置
- 实现 Windows 平台自动检测系统架构并替换旧的架构检测逻辑
- 重构激活/停用相关函数并添加临时文件依赖
- 添加x86架构支持并新增配置类型枚举
- 添加normpath依赖并重构路径处理逻辑，优化README文档
- 新增setup初始化命令，完善项目配置流程

### Other

- *(lib)* 添加全面的单元测试并改进断言
- 代码优化与结构重构
- 重构Windows架构检测逻辑并清理冗余依赖
- 注释掉tips函数和相关调用
- 移除架构自动检测相关废弃功能与代码
- 重构依赖与配置，简化架构检测逻辑
- 重构项目代码结构与功能实现
- *(readme)* 添加致谢部分，提及基于nvm-windows开发

## [0.1.2](https://github.com/Ouka2020/nvm-rs/compare/v0.1.1...v0.1.2) - 2026-09-15

### Other

- 删除README中多余的项目结构章节
- 清理合并冲突，更新配置与依赖

## [0.1.1](https://github.com/Ouka2020/nvm-rs/compare/v0.1.0...v0.1.1) - 2026-09-14

### Other

- *(release-plz)* add release-plz configuration file

## [0.1.0](https://github.com/Ouka2020/nvm-rs/releases/tag/v0.1.0) - 2026-09-14

### Added

- initial commit of nvm-rs

### Other

- 配置并完善项目发布工作流与包元信息
- 重构项目代码结构与依赖管理
- add github.token.txt to gitignore
- 添加release-plz CI工作流和github token示例文件
- 初始化并重构nvm-rs项目基础架构
