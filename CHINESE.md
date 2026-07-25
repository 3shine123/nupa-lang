[-> English](README.md)

<div align="center">
<img src="doc/assets/Nupa_avatar.svg" alt="Nupa_avatar" width="210">

# Nupa 编程语言

[概述](#概述) · [为什么要创造出 Nupa？](#为什么要创造出-nupa) · [快速开始](#快速开始) · [语言特性](#语言特性) · [新特性](#新特性) · [编译与运行](#编译与运行) · [代码示例](#代码示例) · [设计原则](#设计原则) · [路线图](#路线图) · [FAQ](#faq)
</div>

---

## **概述**

Nupa 是一门**纯静态**的 Objective-C 方言（C 超集语言）。Nupa 源码被转译为 C99，再由 Clang 编译为原生机器码。没有运行时消息转发，没有 GC 暂停，没有 JIT 预热——所有方法派发、内存管理、多态都在编译期完成。目前能跑，有小游戏和工具在里面跑着。如果你觉得有意思，可以拿来试试。

我不是想替代 ObjC 或 Swift，只是单纯怀念 ObjC 的语法，想在静态编译的世界里让它再活一次。☺️

---

## 为什么要创造出 Nupa？

我纯粹是喜欢 ObjC 的消息发送语法 `[obj message]`而已。因为 ObjC 的运行时（`objc_msgSend`）太重了，我又想写一段能直接编译成 C 的 ObjC 代码，所以就有了 Nupa ——— 把 ObjC 的语法静态编译掉，不依赖运行时，生成干净的 C。

这不是一个为生产而准备的语言。它是一个玩具，用来探索"如果把 ObjC 转译到静态 C，会是什么样子"。

### 它做了什么

- 方法调用 → 编译期算好 VTable 偏移量，没有 objc_msgSend
- 内存管理 → CFG 静态分析，编译期决定在哪 retain/release
- 生成 C99 → 人能读懂的 C，不是 IR

### 设计目标

- **好玩**：这是最重要的
- **可读**：生成的 C 代码是给人看的
- **轻量**：只有一个静态的迷你运行时

---

## 快速开始

### 依赖

- Clang（>= 14）
- Meson（>= 0.60）
- Ninja
- Git

### 构建

```bash
git clone https://github.com/3shine123/nupa-lang.git
cd nupa-lang
meson setup builddir
ninja -C builddir
```

### 编译一个 Nupa 程序

```bash
# 只输出 C 代码（自动推导 .np → .c）
nupac --rewrite-nupa hello.np
nupac hello.np --rewrite-nupa              # flag 放哪都行
nupac --rewrite-nupa hello.np -o out.c     # 也可以显式指定路径

# 单独用 Clang 编译转译后 C 代码
clang -I include -o hello hello.c -Lbuilddir -lnupa

# 直接输出对象文件
nupac hello.np -o hello.o                  # -c 模式，不链接

# 编译到可执行文件
nupac hello.np -o hello_bin                # 转译 + 编译 + 链接

# 编译 + 运行
nupac run hello.np
nupac run hello.np -o hello_bin            # 运行后保留二进制
nupac run hello.np                          # 运行后自动清理临时文件

# 显示编译警告
nupac -v run hello.np

# [!] 错误：不用 -rewrite-nupa 却输出 .c
nupac hello.np -o hello.c   → Error: use -rewrite-nupa to output C code

# [!] 错误：没有指定任何输出方式
nupac hello.np              → Error: specify -o or -rewrite-nupa
```

### 运行测试

```bash
# 运行所有测试
./test_all.sh -j4

# 运行单个单元测试
./builddir/test_parser
./builddir/test_codegen
等等..
```

---

## 语言特性

### 类系统

```nupa
@interface Animal : NPObject {
@public
    NPString *_name;
}
- (instancetype)initWithName:(NPString *)name;
- (void)speak;
@property (readonly) NPString *name;
@end

@implementation Animal
- (instancetype)initWithName:(NPString *)name {
    self = [super init];
    if (self) {
        _name = name;
    }
    return self;
}
- (void)speak {
    printf("...\n");
}
@end
```

### 协议

```nupa
@protocol Drawable
- (void)draw;
- (BOOL)isVisible;
@end

@interface Shape : NPObject <Drawable>
@end
```

### 属性

```nupa
@interface Person : NPObject
@property NPString *name;
@property int age;
@property (readonly) NPString *identifier;
@end
```

### 类别（Category）

```nupa
@interface Person (Printing)
- (void)printGreeting;
@end

@implementation Person (Printing)
- (void)printGreeting {
    printf("Hello, my name is %s\n", [self name]);
}
@end
```

### Block

```nupa
int (^square)(int) = ^int(int x) {
    return x * x;
};

void (^logAndCall)(NPString *, void (^)(void)) = ^void(NPString *msg, void (^next)(void)) {
    printf("[LOG] %s\n", msg);
    if (next) next();
};
```

### @autoreleasepool

```nupa
@autoreleasepool {
    NPString *temp = [NPString stringWithUTF8String:"hello"];
    // temp 在 pool pop 时自动 release
}
```

### @selector

```nupa
SEL sel = @selector(doSomething:);
```

### C 完全兼容

```nupa
#include <stdio.h>
#include <stdlib.h>

@interface Wrapper : NPObject
- (void)callCFunction;
@end
```

### 新特性

Nupa 在 Objective-C 语法基础上，加入了一些 ObjC 本身没有的语言特性。

#### 隐式根类（__nupa_root）

Nupa 现在支持用户自定义根类。你不再需要强制继承 `NPObject`——不写父类的 `@interface` 会自动获得编译器注入的隐式根类 `__nupa_root`，同时保持 `id` 类型的统一性和静态派发能力。

**之前：**

```nupa
@interface Animal : NPObject   // 必须继承 NPObject
```

**之后：**

```nupa
@interface Animal              // 不写父类 → 隐式根类
@interface Animal : NPObject   // 显式继承 NPObject 仍然合法
```

两者都合法，且 `id` 可以指向任何 Nupa 对象。

#### 核心机制

当用户不写父类时，编译器自动注入 `__nupa_root`：

```nupa
// 用户代码：
@interface Animal {
    int age;
}
- (void)speak;
@end

// 编译器视为：
@interface Animal : __nupa_root {
    int age;
}
- (void)speak;
@end
```

生成的 C 代码：

```c
// 编译器内置结构
struct nupa_object_header {
    struct nupa_vtable *vtable;
};

struct __nupa_root {
    struct nupa_object_header header;
};

// Animal 的 struct
struct Animal {
    struct __nupa_root __super;  // 包含 header
    int age;
};
```

#### id 的新定义

```c
typedef struct __nupa_root *nupa_id_t;
```

`id` 不再绑定任何具体类，只要求对象以 `__nupa_root` 开头：

```nupa
Animal *a = [[Animal alloc] init];
id obj = a;                    // ✅ 合法，Animal 继承自 __nupa_root
[obj speak];                   // 静态派发：obj->header.vtable[...]
```

#### NPObject vs __nupa_root

| 写法                          | 含义                       | 适用场景            |
| --------------------------- | ------------------------ | --------------- |
| `@interface Xxx`            | 隐式继承 `__nupa_root`，最轻量   | 自定义内存布局、内核、嵌入式  |
| `@interface Xxx : NPObject` | 显式继承，获得 retain/release 等 | 用户态应用、需要完整运行时支持 |

```nupa
// 自定义根类：轻量，无引用计数
@interface KernelTask {
    int pid;
    int priority;
}
- (void)run;
@end

// 使用 NPObject：完整功能，自动内存管理
@interface UserModel : NPObject
@property NSString *name;
@end
```

#### 方法派发

所有 Nupa 对象通过统一的 VTable 机制静态派发：

```c
// [obj doSomething:arg]
obj->header.vtable[INDEX_doSomething](obj, arg);
```

编译器为每个选择器分配全局固定索引，所有类的 VTable 在同一位置存储对应方法的函数指针。

#### 内核友好设计

对象头极简，只有 vtable 指针（8 bytes on 64-bit），引用计数由编译期 ARC 静态分析管理，不占用运行时对象空间。

#### 状态

   已实现：

- [x] 隐式根类注入（语义分析阶段）
- [x] `__nupa_root` 和 `nupa_object_header` 的 C 代码生成
- [x] `id` → `nupa_id_t` 的类型映射
- [x] 统一 VTable 索引分配
- [x] 根类/子类 struct 生成
- [x] 单元测试覆盖

#### @namespace 命名空间

`@namespace` 用于组织类、函数、常量等代码实体，避免全局命名冲突。这是 ObjC 没有的特性——在传统 ObjC 中需要用前缀（如 `NS`、`UI`）来模拟。

```nupa
@namespace Game {
    @interface Player : NPObject {
        int health;
    }
    - (id)init;
    - (int)getHealth;
    @end

    @implementation Player
    - (id)init {
        self = [super init];
        if (self) health = 100;
        return self;
    }
    - (int)getHealth { return health; }
    @end
}

@namespace UI {
    @interface HUD : NPObject {}
    - (void)showPlayerHealth:(Game::Player *)player;
    @end
}
```

**编码规则**：命名空间通过 `::` 分隔，转译为 C 时使用 `__` 编码。

| Nupa 符号                   | 转译后的 C 符号                    |
| ------------------------- | ---------------------------- |
| `Game::Player`            | `nupa_Game__Player`          |
| `Game::Entities::Enemy`   | `nupa_Game__Entities__Enemy` |
| 方法 `-[Game::Player init]` | `Game__Player_init`          |
| VTable                    | `nupa_Game__Player_vtable`   |
| 类元数据                      | `nupa_Game__Player_class`    |

**特性**：

- 支持嵌套命名空间（`Game::Entities::Enemy`）
- 支持跨命名空间引用（`Game::Player *player`）
- 无需前缀约定，编译器自动编码 C 符号
- 无命名空间的类保持向后兼容

#### @using 导入机制

`@using` 用于将其他命名空间中的符号导入当前作用域，避免每次使用都写完整限定名。支持三种形式：

**形式一：导入完整限定名**

```nupa
@using Game::Player;
Game::Player *p = [[Game::Player alloc] init];
// 可以直接用 Player 代替 Game::Player
Player *p = [[Player alloc] init];
```

**形式二：导入并指定别名**

```nupa
@using GP = Game::Player;
// 用 GP 作为 Game::Player 的别名
GP *p = [[GP alloc] init];
```

**形式三：导入整个命名空间**

```nupa
@using namespace Game;
// Game 命名空间下的所有类可以直接用短名访问
Player *p = [[Player alloc] init];
Enemy *e = [[Enemy alloc] init];
```

**冲突检测**：

- 如果短名与当前作用域已有符号冲突，编译器报错
- 如果两个 @using 条目导入相同的短名，编译器报二义性错误
- 别名和短名在 `@using` 声明所在的文件作用域内有效

---

## 编译与运行

### 命令行选项

```bash
nupac [options] <input.np>

模式:
  (无)              默认：转译 + 编译到二进制（需要 -o）
  run               转译 + 编译 + 运行（自动清理临时文件）

选项:
  -o <file>         指定输出文件（.o 输出对象文件，否则输出可执行文件）
  -H <header.h>     从 .nh 生成头文件
  -I <dir>          添加头文件搜索路径
  -L <dir>          添加库搜索路径
  -v, --verbose     显示详细输出（包括 Clang 编译警告）
  --version         显示版本号
  --rewrite-nupa    只输出 C 代码（不编译）
  -fno-nupa-arc     禁用 ARC（手动 MRC 模式）
```

### 构建系统集成

**cargo**:

```bash
cargo build
cargo test --workspace
```

---

## 代码示例

### Hello World

```nupa
#include <stdio.h>
#import <Foundation/Foundation.nh>

@interface Greeter : NPObject
- (void)greet;
@end

@implementation Greeter
- (void)greet {
    printf("Hello, Nupa!\n");
}
@end

int main() {
    @autoreleasepool {
        Greeter *g = [[Greeter alloc] init];
        [g greet];
    }
    return 0;
}
```

### 多态

```nupa
@interface Animal : NPObject
- (void)speak;
@end

@interface Dog : Animal
@end

@interface Cat : Animal
@end

@implementation Animal
- (void)speak { printf("...\n"); }
@end

@implementation Dog
- (void)speak { printf("Woof!\n"); }
@end

@implementation Cat
- (void)speak { printf("Meow!\n"); }
@end

int main() {
    Animal *animals[2];
    animals[0] = [[Dog alloc] init];
    animals[1] = [[Cat alloc] init];
    for (int i = 0; i < 2; i++)
        [animals[i] speak];  // VTable 静态派发
    return 0;
}
```

### Block + ARC

```nupa
typedef void (^EventHandler)(int code, NPString *msg);

@interface Engine : NPObject
- (void)onEvent:(EventHandler)handler;
@end

int main() {
    @autoreleasepool {
        Engine *e = [[Engine alloc] init];
        int captured = 42;
        [e onEvent:^void(int code, NPString *msg) {
            printf("code=%d msg=%s captured=%d\n", code, msg, captured);
        }];
    }
    return 0;
}
```

### 静态泛型（Generics）

Nupa 通过**编译期单态化（monomorphization）**实现泛型——每个 `DataPack<QuantumToken *>` 都会生成独立的 C 结构体 `DataPack_QuantumToken_ptr`，类型参数被具体类型替换。没有类型擦除，没有装箱，没有运行时开销。

```nupa
@interface DataPack<T> : NPObject {
    @public
    int _count;
    T _storage[2];
}
- (void)pushItem:(T)item;
- (T)popItem;
@end

@implementation DataPack
- (void)pushItem:(T)item {
    if (_count < 2) {
        _storage[_count++] = item;
    }
}
- (T)popItem {
    if (_count > 0) {
        _count--;
        T item = _storage[_count];
        _storage[_count] = 0;
        return nupa_autorelease(item);
    }
    return 0;
}
@end

int main() {
    @autoreleasepool {
        // 每次实例化都会生成特化的 C 代码
        DataPack<QuantumToken *> *tokenPack = [[DataPack<QuantumToken *> alloc] init];
        DataPack<EncryptedMetric *> *metricPack = [[DataPack<EncryptedMetric *> alloc] init];
    }
    return 0;
}
```

**工作原理**：

- `DataPack<T>` 的泛型结构体不会被发射；只发射特化版本
- `DataPack<QuantumToken *>` → `struct DataPack_QuantumToken_ptr`，包含 `QuantumToken * _storage[2]`
- 方法会被克隆，返回类型、参数类型、方法体中的类型参数全部被替换
- VTable、元 VTable、类元数据均为每个实例化单独生成
- 类型名编码：`DataPack<QuantumToken *>` → `DataPack_QuantumToken_ptr`

**状态**：✅ 单类型参数完整实现。多参数（`<K, V>`）开发中。

---

## 设计原则

### 1. 静态 > 动态

ObjC 的运行时很强大，但我不想依赖它。把所有决策放在编译期，生成就是执行，没有意外。

- 方法派发 → VTable 偏移量
- 内存管理 → CFG 静态分析
- 协议一致性 → 编译期检查

### 2. 生成人能读的 C

Nupa 的"后端"是**人类可读的 C99**，不是 LLVM IR。这意味着：

- 可以用 Clang/LLDB 原生工具调试
- 生成的 C 可以审查、修改、嵌入到其他项目
- 没有 LLVM 后端绑定——Clang 能跑的地方 Nupa 就能跑

### 3. 渐进式

从一个类系统开始，慢慢加东西：

- ✅ 类/协议/类别/属性
- ✅ Block / @autoreleasepool
- ✅ 静态 ARC
- ✅ @selector / VTable 多态
- ⏳ Foundation 标准库
- ⏳ 异常处理
- ⏳ 编译器自举

### 5. 可读性

生成的 C 代码应该像手写的 C 一样清晰：

- 使用 `struct` + `->` 访问 ivar
- 使用 `static const SEL` 常量
- 方法和变量命名一致且可预期
- 临时变量显式命名

---

## 路线图

### 阶段 1：基础设施 ✅

- [x] 词法分析器
- [x] 预处理器
- [x] 语法分析器
- [x] CST 验证与打印

### 阶段 2：语义分析 ✅

- [x] 符号表
- [x] 名称绑定
- [x] 类型检查
- [x] 属性展开
- [x] 协议一致性

### 阶段 3：VTable + 对象布局 ✅

- [x] VTable 布局
- [x] 对象内存布局
- [x] 类元数据

### 阶段 4：中间表示 ✅

- [x] Typed AST
- [x] CST → AST
- [x] CFG 构建

### 阶段 5：静态 ARC ✅

- [x] Ownership 推断
- [x] 局部 + 全局 ARC
- [x] Retain/Release 插入
- [x] ARC 验证

### 阶段 6：C99 代码生成 ✅

- [x] C99 AST
- [x] AST → C99 转换
- [x] 头文件生成
- [x] 编译器选项

### 阶段 7：运行时 ✅

- [x] 核心 retain/release/alloc/init
- [x] 自动释放池

### 阶段 8-10：进行中

- [ ] Foundation 标准库

- [x] Block 运行时

- [x] 弱引用

- [✅] 泛型（编译期单态化）

- [ ] 异常处理

- [ ] 调试信息

- [x] VSCode/IDE 支持

---

## FAQ

### 它能用于生产环境吗？

还不能。但它是 **真实可用** 的 —— 它能编译、能运行，并且从一开始就是为成长而设计的。如果你觉得它的语法很对味，想给它贡献一下，那么非常欢迎。

### Nupa 能做什么？

写小游戏、写工具、写玩具。项目里的贪吃蛇、Flappy Bird、太空射击、井字棋都是 Nupa 写的，跑在终端里。

### 和 ObjC 比少了什么？

- 没有 `objc_msgSend`——VTable 静态派发
- 没有运行时 Method Swizzling
- 没有 `forwardInvocation:`
- 选择器是编译期常量，不是运行时字符串

### 为什么用 C99 作为输出？

因为 C99 到处都能编译。生成人能读懂的 C，用 Clang 编译，用 lldb 调试。不需要绑定任何特定后端。

---

## 许可证

MIT License

Copyright (c) 2026 3shine123