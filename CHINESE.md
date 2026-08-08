[-> English](README.md)

<div align="center">
<img src="doc/assets/Nupa_avatar.svg" alt="Nupa_avatar" width="210">

# Nupa 编程语言

[**查看项目示例**](#项目示例)

[概述](#概述) · [为什么要创造出 Nupa？](#为什么要创造出-nupa) · [项目示例](#项目示例) · [快速开始](#快速开始) · [语言特性](#语言特性) · [新特性](#新特性) · [编译与运行](#编译与运行) · [代码示例](#代码示例) · [设计原则](#设计原则) · [路线图](#路线图) · [FAQ](#faq)

</div>

---

> ⚠️注意
> **GitHub 网页上的发行版（Release）一般比 push 上来的源码旧。** 仓库里展示的
> Release 常常落后于 `main` 分支的代码——我经常 push 完新提交就忘了发新版本。
> 想要最新特性和修复，请 **clone 仓库自行从源码构建**（见 [构建](#构建)）；
> 只有当你更愿意用较旧但稳定的快照时，才下载 Release。

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

## 项目示例

[![examples](https://img.shields.io/badge/examples-000?style=for-the-badge)](examples/)

`examples/` 

| 项目                    | 说明                                                                      | 运行                            |
| --------------------- | ----------------------------------------------------------------------- | ----------------------------- |
| **`04_soma-kernel/`** | 很小的 32 位 i386 操作系统内核（NASM + C + Nupa），裸机 `-fno-libc` 模式                 | `./run.sh` 或 `./run.sh --gui` |
| **`03_LibUI/`**       | 基于 [libui-ng](https://github.com/libui-ng/libui-ng) 的 GUI 应用，全部回调纯 Nupa | `./run_libui.sh`              |
| **`02_ncurses/`**     | 终端示例（`ncurses_demo`、`sysmon`），使用 `Terminal::Ncurses` 绑定                 | `make run`                    |
| **`01_JSONEditor/`**  | 多文件 JSON 编辑器，分屏终端预览                                                     | `nupac run json_editor.np`    |

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
cargo build --release
```

### 安装

构建完成后，`nupac` 同目录下会自动生成 `install.sh`（以及头文件和 `libnupa.a`）。直接运行它即可安装到系统：

```bash
# 源码编译后——脚本就在二进制旁边
cd target/release        # 或 target/debug（如果你跑的是 cargo build）
./install.sh             # 默认安装到 /opt/nupa
./install.sh /usr/local  # 可选：换成其他前缀
```

脚本会安装：
- **二进制** → `<prefix>/bin/nupac`
- **静态库** → `<prefix>/lib/libnupa.a`
- **头文件** → `<prefix>/include/`
- **系统头文件** → `/usr/local/include/{Foundation,nupa}/`（需写权限；无权限时自动跳过，可用 sudo 重试，或传第二个参数指定目录，如 `./install.sh /opt/nupa ~/include`）

安装脚本会自动检测系统语言（中文 / English）。

或者下载预编译的 Release 压缩包（`nupa-<platform>.tar.gz` 或 `.zip`），解压后运行里面的 `install.sh`：

```bash
tar xzf nupa-x86_64-unknown-linux-musl.tar.gz
cd nupa-x86_64-unknown-linux-musl
./install.sh
```

> **提示：** 把 `nupac` 加入 PATH 并装好系统头文件后，`<nupa/runtime.h>` 和 `<Foundation/...>` 会自动被找到，无需手动加 `-I include`。

### 编译一个 Nupa 程序

```bash
# 只输出 C 代码（自动推导 .np → .c）
nupac -rewrite-nupa hello.np
nupac hello.np -rewrite-nupa               # flag 放哪都行
nupac -rewrite-nupa hello.np -o out.c      # 也可以显式指定路径
# （双横线 --rewrite-nupa 形式同样接受）

# 单独用 Clang 编译转译后 C 代码（两种方式）：
#   1) 直接编译运行时源码
clang -I include -o hello hello.c include/nupa/runtime.c
#   2) 链接编译好的 libnupa.a（位于 nupac 二进制同目录）
clang -I include -o hello hello.c -Ltarget/release -lnupa

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

### Shell 补全（Tab 自动补全）

`nupac` 自带用 [clap_complete](https://crates.io/crates/clap_complete) 生成的 **zsh / bash / fish** 补全脚本。随时可用以下命令重新生成：

```bash
nupac -gen-completions zsh > _nupac
nupac -gen-completions bash > nupac.bash
nupac -gen-completions fish > nupac.fish
```

`install.sh` 也会把脚本装进安装包（`share/nupac/completions/`）。

**zsh** —— 把目录加进 `fpath`（必须在 `compinit` 之前）：

```zsh
fpath=(/opt/nupa/share/nupac/completions $fpath)
autoload -U compinit && compinit
```

**bash**：

```bash
source /opt/nupa/share/nupac/completions/nupac.bash
```

**fish**：

```fish
source /opt/nupa/share/nupac/completions/nupac.fish
```

装完新版本后清一下 zsh 缓存：`rm -f ~/.zcompdump*`，再开新终端。

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

### C 属性（__attribute__）

Nupa 支持 `__attribute__((...))` 透传。你可以在全局声明和 struct 字段上直接写 C 的 `__attribute__`，编译器会把它们原样保留到生成的 C 代码中。

```nupa
__attribute__((packed))
struct Point {
    int x;
    int y;
};

__attribute__((format(printf, 1, 2)))
int my_log(const char *fmt, ...);
```

编译器内置了一个 **590 个属性的三分类表**（来源：Clang 和 GCC 官方文档）。`-backend` 选项控制允许使用哪些属性：

| 选项 | 行为 |
|---|---|
| `-backend=portable`（默认） | 只允许 gcc 和 clang 都支持的属性，其余报错 |
| `-backend=clang` | 允许 clang 专属属性（如 `availability`、`diagnose_if`、`objc_direct`） |
| `-backend=gcc` | 允许 gcc 专属属性（如 `strub`、`optimize`、`stack_protect`） |

不在表中的未知属性只产生 warning 并透传，绝不会报错。

### 新特性

Nupa 在 Objective-C 语法基础上，加入了一些 ObjC 本身没有的语言特性。

**近期亮点：**

- **原生裸机支持（`-fno-libc`）** — 编译为自包含 C，无 libc、无 Foundation、无 TLS；`@try/@catch` 用 `__builtin_setjmp/longjmp`，零样板的 `runtime_baremetal.c` 提供 bump allocator、`NUPA_CLASS_$_nupa_root`、异常状态和 `memcpy`。
- **C 超集** — `@protocol` + 一致性检查、`@property` + `@synthesize`、`instancetype`、`@public` ivar、点语法、struct + 函数指针、内联汇编、C 风格类型转换。
- **类型化 `@catch`** — 每个 catch 块现在检查 `isa == &NUPA_CLASS_$_Class`，只有匹配的类才进入该处理器；多个 catch 正确隔离。
- **ARC 修复** — 作用域栈模型不再在嵌套作用域结束时释放父作用域变量；`for` 初始化对象提升修复了泄漏和非法 `for` 头。
- **`__attribute__` 透传 + `-backend`** — 完整支持 C 的 `__attribute__((...))` 和所有 `__` 前缀的 C 预定义标识符（`__FILE__`、`__LINE__`、`__builtin_*`、`__extension__`、`__typeof__`、`__alignof__` 等）；`-backend` 选项控制哪些编译器专属属性允许使用。

#### 隐式根类（nupa_root）

Nupa 现在支持用户自定义根类。你不再需要强制继承 `NPObject`——不写父类的 `@interface` 会自动获得编译器注入的隐式根类 `nupa_root`，同时保持 `id` 类型的统一性和静态派发能力。

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

当用户不写父类时，编译器自动注入 `nupa_root`：

```nupa
// 用户代码：
@interface Animal {
    int age;
}
- (void)speak;
@end

// 编译器视为：
@interface Animal : nupa_root {
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

struct nupa_root {
    struct nupa_object_header header;
};

// Animal 的 struct
struct Animal {
    struct nupa_root __super;  // 包含 header
    int age;
};
```

#### id 的新定义

```c
typedef struct nupa_root *nupa_id_t;
```

`id` 不再绑定任何具体类，只要求对象以 `nupa_root` 开头：

```nupa
Animal *a = [[Animal alloc] init];
id obj = a;                    // ✅ 合法，Animal 继承自 nupa_root
[obj speak];                   // 静态派发：obj->header.vtable[...]
```

#### NPObject vs nupa_root

| 写法                          | 含义                       | 适用场景            |
| --------------------------- | ------------------------ | --------------- |
| `@interface Xxx`            | 隐式继承 `nupa_root`，最轻量   | 自定义内存布局、内核、嵌入式  |
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

#### 裸机 / Freestanding 支持（`-fno-libc`）

Nupa 可以编译为**无 libc、无 Foundation、无 TLS** 的自包含 C，直接用于内核、MCU、嵌入式裸机开发。

```bash
nupac -rewrite-nupa -fno-libc kernel.np   # 生成自包含 C
```

`-fno-libc` 模式下转译出的 C：

- 不 `#include <string.h>`，改 `#include <nupa/runtime.h>`（freestanding 分支）
- `@try/@catch/@finally` 用 `__builtin_setjmp/longjmp`（零 libc），异常状态用普通全局而非 `__thread`
- 类型（`SEL`/`NPClass`/`NPObject`/`id`）自含

用户只需提供：`nupa_nupa_root_class`、异常全局（如用 `@try`）、`memcpy`（如用 `@try`）、freestanding 头（`stdint.h`/`stddef.h`/`stdbool.h`）。

**裸机分配器 + `[[Class alloc] init]`**（`include/nupa/runtime_baremetal.c`）：

```nupa
@interface HeapCounter {
    int total;
}
+ (id) alloc;
- (id) init;
- (int) add:(int)x;
@end
@implementation HeapCounter
+ (id) alloc  { return nupa_alloc(self); }   // bump allocator
- (id) init   { return self; }
- (int) add:(int)x { total += x; return total; }
@end

void demo(void) {
    HeapCounter *c = [[HeapCounter alloc] init];  // 裸机堆分配
    [c add:10];                                    // → 10
}
```

已跑通的特性（`examples/04_soma-kernel/` 的 i386 保护模式内核 + `tests/golden/25_freestanding/`）：

- `@namespace` + `@interface`（隐式根类）
- 类方法 / 实例方法消息派发
- `@try/@catch/@finally`
- `@selector`、内联 asm、C 类型转换
- `[[Class alloc] init]` 裸机堆分配 + ARC 自动 `nupa_release`

运行示例（soma-kernel 在 qemu 下）：

```
[nupa] class method [SomaCore::Calculator compute:21] = 43
[nupa] instance methods on C-created obj: add:7 -> 7, add:35 -> 42, value = 42
[nupa] @try/@catch demo:
       try body, throwing...
       caught [e errorCode] = 42
       finally always runs
       after-try continues
[nupa] alloc+init (bump allocator):
       [c add:10]=10 [c add:20]=30 [c value]=30
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
- [x] `nupa_root` 和 `nupa_object_header` 的 C 代码生成
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
| `Game::Player`            | `Game__Player`          |
| `Game::Entities::Enemy`   | `Game__Entities__Enemy` |
| 方法 `-[Game::Player init]` | `Game__Player_init`          |
| VTable                    | `NUPA_VTABLE_$_Game__Player`   |
| 类元数据                      | `NUPA_CLASS_$_Game__Player`    |

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
  -I <dir>          添加头文件搜索路径
  -L <dir>          添加库搜索路径
  -v, --verbose     显示详细输出（包括 Clang 编译警告）
  --version         显示版本号
  --rewrite-nupa    只输出 C 代码（不编译）
  -fno-nupa-arc     禁用 ARC（手动 MRC 模式）
  -fno-checker      跳过类型检查
  -fno-libc         裸机/freestanding 输出（无 libc、无 TLS）
  -backend <mode>   C 编译器后端：portable（默认）、clang、gcc
  -arch <target>    构建目标架构（如 -arch x86_64）
  -asm <file.s>     链接汇编文件（可重复）
  -gen-completions <shell>  生成 shell 补全脚本（zsh|bash|fish）
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