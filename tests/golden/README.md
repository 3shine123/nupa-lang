# Nupa 黄金测试集

按功能分类的系统化回归测试。每个测试包含 `.np`（源码）和 `.out`（期望输出）文件。

## 目录结构

```
01_basics/          基础功能（hello world, 纯 C 互操作）
02_class/           类系统（继承、方法调用、self/super）
03_properties/      属性（getter/setter、dot syntax、@synthesize、@dynamic）
04_arc/             ARC 内存管理（retain/release、作用域、分支、dealloc、weak）
05_autoreleasepool/ 自动释放池（简单/顺序池、return、循环）
06_polymorphism/    多态（vtable 动态分发、id 类型）
07_protocols/       协议（声明、合规、编译失败测试）
08_categories/      分类（命名分类、扩展）
09_blocks/          Block 语法（字面量、变量捕获）
10_edge_cases/      边界情况（nil、instancetype、@class、@selector 等）
22_c_superset/       C 超集语法（struct、C 风格 cast、函数指针）
23_asm/              Inline asm（extended asm、命名操作数、asm goto）
24_asm_fusion/       asm 融合压测（内联+外部 asm × 类/协议/Block/异常/struct/fn-ptr）
```

> x86_64 汇编是跨架构用例，不放入默认 arm64 测试套件，单独位于 `asm_x64/`（见下文）。

## x86_64 / Rosetta 测试

在 arm64 Mac 上通过 Rosetta 运行 x86_64 汇编：

```bash
nupac -arch x86_64 run -asm asm_x64/asm_x86_ext.s asm_x64/asm_x86_fusion_test.np
# 或
./asm_x64/build.sh
```

要点：
- 使用系统原生的 `/usr/bin/arch`（不是 uutils 的 `arch`，后者没有 `-x86_64` 切换能力）。
- 未安装 Rosetta 时先 `softwareupdate --install-rosetta --agree-to-license`。
- `-arch x86_64` 让 clang 交叉产出 x86_64 Mach-O，执行时被 Rosetta 自动翻译。
- x86_64 内联 asm 用 `%r` 即可（32 位操作数直接用 eax 等），无需 ARM64 的 `%w` 修饰符。
- Rosetta 安装后若立即报 "Bad CPU type in executable"，等 oahd 激活后重试。

## 运行方式

```bash
# 方式 1: 顶层脚本
./test_golden.sh

# 方式 2: 通过 test_all.sh（--golden 标志）
./test_all.sh --golden

# 方式 3: 直接运行 Python runner
python3 test/golden/test_golden.py

# 方式 4: Shell runner
./test/golden/run_golden.sh
```

## 选项

| 参数 | 说明 |
|------|------|
| `-jN` | 并行 N 任务 |
| `-fsemantic` | 启用语义检查 |
| `-fno-micrit-arc` | 禁用 ARC |
| `--update` | 用实际输出更新 .out 文件 |
| `-v` | 详细输出（仅 shell runner） |

## 添加新测试

1. 在对应分类目录下创建 `test_name.np`
2. 运行测试，用 `--update` 生成 .out 文件
3. 验证输出正确后提交

## 特殊文件

- `protocol_fail.np` — 无 `.out` 文件，预期编译失败
- 其他所有 `.np` 文件必须对应一个 `.out` 文件

## 当前状态

运行 `./test_golden.sh` 查看当前通过/失败情况。
