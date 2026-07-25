# Nupa → C99 代码生成规范（CodeGen Spec）

> 版本：2026-07-25  
> 适用范围：`nupac` 编译器后端（`codegen.c` / `codegen.rs` / `codegen_emit.c`）

---

## 1. 总体原则

1. **生成的人类可读 C 代码是产物之一**，不是中间垃圾。变量名、缩进、结构必须清晰。

2. **Clang 扩展可用**：允许使用 Blocks（`-fblocks`）、匿名结构体初始化、复合表达式（`({ ... })`），但**不依赖 GCC 专属扩展**。

3. **C99 标准兼容**：所有生成代码必须能通过 `clang -std=c99 -fblocks -Werror`。

4. **零运行时魔法**：所有语义必须在转译阶段解决，生成的 C 代码不依赖解释器或 JIT。

---

## 2. 对象头与结构体规则

### 2.1 根类结构

c

```c
#ifndef __NUPA_ROOT_DEFINED
#define __NUPA_ROOT_DEFINED
struct __nupa_root {
    struct NPClass *isa;      // ✅ 必须使用 struct NPClass *，禁止用 NPClass * typedef
    uint32_t retain_count;
};
typedef struct __nupa_root __nupa_root;
#endif
```

### 2.2 普通类结构

c

```c
struct Engine__Math__Vector2D {
    struct NPClass *isa;      // ✅ 统一使用 struct NPClass *
    uint32_t retain_count;
    int x;
    int y;
};
typedef struct Engine__Math__Vector2D Engine__Math__Vector2D;
```

### 2.3 结构体生成约束

- **所有类结构体**（含 `__nupa_root`、`NPObject`、用户类）的 `isa` 字段**必须**使用 `struct NPClass *isa`，**禁止**使用 `NPClass *isa`。

- `retain_count` 紧跟 `isa` 之后。

- ivar 按声明顺序排列，父类 ivar 已在父类结构体中，子类只追加自己的 ivar。

- 每个 `struct` 后必须紧跟 `typedef struct Name Name;`。

---

## 3. 命名空间编码规则

表格

| Nupa 语法                   | C 符号                                              |
| ------------------------- | ------------------------------------------------- |
| `Game::Player`            | `Game__Player`（`::` → `__`）                       |
| `Game::Entities::Enemy`   | `Game__Entities__Enemy`                           |
| 方法 `-[Game::Player init]` | `Game__Player_init`                               |
| VTable 实例                 | `nupa_Game__Player_vtable_inst`                   |
| Meta VTable 实例            | `nupa_Game__Player_meta_vtable_inst`              |
| 类元数据                      | `nupa_Game__Player_class`                         |
| Block typedef             | `Extension__ActionCompleteBlock`（**必须**带完整命名空间前缀） |

### 3.1 Block typedef 规范

- **禁止**生成无前缀的原始 Block 类型名。

- 错误：
  
  c
  
  ```c
  typedef void (^ActionCompleteBlock)(...);          // ❌
  typedef ActionCompleteBlock Extension__ActionCompleteBlock; // ❌ 别名模式
  ```

- 正确：
  
  c
  
  ```c
  typedef void (^Extension__ActionCompleteBlock)(int, Engine__Math__Vector2D *);
  ```

---

## 4. 方法签名与 VTable 规则（绝对禁止修改）

### 4.1 方法实现函数签名

**所有**实例方法的 `self` 参数和返回类型**必须**保持以下形式：

c

```c
NPObject * Engine__Math__Vector2D_initWithX_y_(NPObject * self, SEL _cmd, int posX, int posY);
void Engine__Graphics__RenderNode_dealloc(NPObject * self, SEL _cmd);
```

- `self` **必须**是 `NPObject *`，**禁止**改为具体子类指针（如 `Engine__Math__Vector2D *self`）。

- 返回类型**必须**是 `NPObject *`（或 `void` / 基础类型），**禁止**返回具体子类指针。

- 这条规则**不可动摇**，它是 Uniform VTable 和对象头极简设计的基础。

### 4.2 VTable 结构

使用**统一的** `struct nupa_vtable`，**禁止**为每个类生成独立的 vtable struct：

c

```c
struct nupa_vtable {
    void (*addVector_)(NPObject *, SEL, Engine__Math__Vector2D *);
    NPObject * (*init)(NPObject *, SEL);
    // ... 所有方法按全局固定索引排序
};
```

- 每个类的 vtable 实例是 `struct nupa_vtable nupa_ClassName_vtable_inst = { ... };`。

- 未实现的方法槽位填 `NULL`。

- Meta VTable 保持独立结构（`struct nupa_ClassName_meta_vtable`），因为类方法签名不同（接收 `NPClass *`）。

---

## 5. 类型强制转换规则（重点）

### 5.1 必须插入显式强制转换的场景

当**目标类型**是具体子类指针，而**源表达式**类型是 `NPObject *`（或 `id` / `__nupa_root *`）时，必须插入 `(TargetClassName *)`。

#### 场景 A：局部变量声明初始化

c

```c
// 正确
Engine__Math__Vector2D * offset1 = (Engine__Math__Vector2D *)(((struct nupa_vtable *)...)->initWithX_y_(...));

// 错误（缺少强制转换）
Engine__Math__Vector2D * offset1 = ((struct nupa_vtable *)...)->initWithX_y_(...);
```

#### 场景 B：ivar / 属性赋值

c

```c
// 正确
((struct Engine__Graphics__RenderNode *)self)->_position = 
    (Engine__Math__Vector2D *)(({
        NPObject *__nupa_tmp_1 = ...;
        __nupa_tmp_1 ? ... : 0;
    }));

// 错误（缺少强制转换）
...->_position = ({ ... });  // ❌
```

#### 场景 C：方法调用实参（如果形参是 NPObject *，不需要转）

- 如果形参已经是 `NPObject *`，实参不需要 `(NPObject *)` 转换。

- 但如果形参是具体子类指针（如 `Engine__Graphics__RenderNode *`），实参是 `NPObject *` 时**需要**转换。

### 5.2 禁止过度转换

- 如果变量**已经是**目标类型，再次传入同类型形参时**不需要**重复转换。

- 示例（冗余，应避免）：
  
  c
  
  ```c
  ((struct nupa_vtable *)(manager->isa->vtable))->registerNode_((NPObject *)(manager), ...);
  // manager 已经是 Gameplay__GameManager *，但形参是 NPObject *，此处 (NPObject *) 是冗余的
  ```
  
  虽然冗余不影响编译，但应只在**真正需要**时插入转换。

---

## 6. 表达式与语句转换规则

### 6.1 消息发送 `[]`

- 所有消息发送必须转译为 VTable 静态派发：
  
  c
  
  ```c
  ((struct nupa_vtable *)recv->isa->vtable)->methodName(recv, sel, args...)
  ```

- 如果 receiver 可能是 `nil`，必须包装为 nil-safe：
  
  c
  
  ```c
  ({ NPObject *__nupa_tmp_N = (expr); __nupa_tmp_N ? ((struct nupa_vtable *)__nupa_tmp_N->isa->vtable)->method(__nupa_tmp_N, sel, ...) : 0; })
  ```

- 临时变量命名：`__nupa_tmp_1`、`__nupa_tmp_2`，按函数内递增。

### 6.2 `@selector`

- 生成 `sel_registerName("methodName")` 调用，**禁止**直接输出 FNV-1a hash 作为无符号整数。

- 或输出静态常量：
  
  c
  
  ```c
  static const SEL __nupa_sel_init = {.name = "init", .hash = 0x16B1D373};
  ```

### 6.3 `@try` / `@catch` / `@finally` / `@throw`

- 使用 `setjmp` / `longjmp` + TLS 全局变量实现：
  
  c
  
  ```c
  // @try
  jmp_buf __nupa_try_buf;
  if (setjmp(__nupa_try_buf) == 0) {
      // try body
  } else {
      // @catch (id e) → 从 __nupa_exception_value 读取
  }
  // @finally 块放在最后，确保无论是否异常都执行
  ```

### 6.4 `@autoreleasepool`

c

```c
nupa_autoreleasepool_t * __nupa_pool = nupa_autoreleasepool_push();
// body
nupa_autoreleasepool_pop(__nupa_pool);
```

### 6.5 `super` 调用

- 直接通过**父类 vtable 实例**调用，不走虚派发：
  
  c
  
  ```c
  self = (&nupa_NPObject_vtable_inst)->init(self, __nupa_sel_init);
  ```

### 6.6 点语法（属性访问）

- 如果属性有已知 ivar 且是 `readonly` 或简单访问，直接内联为 `self->ivar`。

- 否则走 VTable getter/setter 派发。

---

## 7. 头文件与包含规则

### 7.1 标准头文件

- 去重输出，**禁止**同一头文件出现多次。

- 标准顺序：
  
  c
  
  ```c
  #include <stdio.h>
  #include <stdlib.h>
  #include <string.h>
  #include <stdint.h>
  #include <stddef.h>
  #include "nupa/runtime.h"
  ```

- 如果源码中有 `#include <string.h>` 和 `#import <Foundation/Foundation.nh>`，生成器应去重，且标准库头文件只输出一次。

### 7.2 前向声明

- 所有用户定义类结构体必须在文件头部前向声明：
  
  c
  
  ```c
  struct Engine__Math__Vector2D;
  typedef struct Engine__Math__Vector2D Engine__Math__Vector2D;
  ```

---

## 8. Block 生成规则

### 8.1 Block 类型定义

- 使用带命名空间前缀的 typedef（见 3.1）。

- 参数类型如果是对象指针，使用具体类指针类型（如 `Engine__Math__Vector2D *`）。

### 8.2 Block 字面量

- 生成 Block 结构体 + invoke 函数。

- `__block` 变量生成 `struct __nupa_byref_varName`：
  
  c
  
  ```c
  struct __nupa_byref_counter {
      void *__isa;
      struct __nupa_byref_counter *__forwarding;
      int __flags;
      int __value;
  };
  ```

---

## 9. 泛型（Monomorphization）规则

- 泛型类（如 `DataPack<T>`）**不生成**通用结构体，只为每个实例化生成特化版本。

- 特化结构体命名：`DataPack_QuantumToken_ptr`。

- 方法同样克隆并替换所有 `T` 为具体类型。

- VTable、Meta VTable、类元数据均按特化版本独立生成。

---

## 10. 内存管理相关生成

### 10.1 MRC 模式（`-fno-nupa-arc`）

- 用户手动写 `retain` / `release` / `autorelease`，代码生成器**原样转译**为对应的 C 函数调用。

### 10.2 ARC 模式（默认）

- 编译器在 AST/CFG 层分析所有权，**自动插入** `nupa_retain()` / `nupa_release()`。

- 生成的 C 代码中**不应出现**用户手写的 `retain` / `release`（除非在 `-fno-nupa-arc` 模式）。

- `dealloc` 方法中 `[super dealloc]` 必须保留，且生成为父类 vtable 的 `dealloc` 调用。

---

## 11. 禁止事项清单

表格

| #   | 禁止行为                                      | 原因                                           |
| --- | ----------------------------------------- | -------------------------------------------- |
| 1   | 将方法签名中的 `NPObject *self` 改为具体子类指针         | 破坏 Uniform VTable 和对象头一致性                    |
| 2   | 将 `struct NPClass *isa` 改为 `NPClass *isa` | 类型不一致，依赖 typedef 可见性                         |
| 3   | 生成重复的 `#include`                          | 冗余，可能引发宏重定义问题                                |
| 4   | Block typedef 不带命名空间前缀                    | 全局命名空间污染                                     |
| 5   | `NPObject *` → 子类指针赋值时不加显式强制转换            | `clang -Werror` 报 incompatible pointer types |
| 6   | 为每个类生成独立的 vtable struct                   | 破坏 Uniform VTable 设计                         |
| 7   | `@selector` 直接输出 hash 值作为无符号整数            | `SEL` 是结构体，不是 `unsigned`                     |
| 8   | 在 `@try` body 中生成空块或 `/* stub */`         | 异常处理必须实际生效                                   |

---

## 12. 验收测试用例

修改 codegen 后，必须验证以下文件能正确转译并通过 `clang -std=c99 -fblocks -Werror`：

1. `absolute_qualified_hardcore_test.np` —— 深层命名空间、跨空间继承、Block、@try/@catch

2. `grand_integrated_epic_test.np` —— 命名空间、VTable 多态、属性、@autoreleasepool

3. `log_analyzer.np` —— 协议、泛型容器、双 Block 回调、统计报告
