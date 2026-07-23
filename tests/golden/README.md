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
```

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
