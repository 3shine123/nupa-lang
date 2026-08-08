# Nupa 命名规范（Nupa Naming Convention）

> 对标 Objective-C Runtime 命名体系。Nupa 有的必须跟 ObjC 的分类和命名方式一致，没有的坚决不引入。

---

## 1. 分类体系总览

| 类别 | ObjC 例子 | Nupa 例子 | 命名规则 |
|---|---|---|---|
| 框架类 | `NSObject`, `NSString` | `NPObject`, `NPString` | CamelCase + `NP` 前缀 |
| 运行时 C API | `objc_msgSend`, `objc_getClass` | `nupa_release`, `nupa_alloc` | snake_case + `nupa_` 前缀 |
| 运行时类型 | `objc_object`, `objc_class` | `nupa_vtable`, `nupa_root` | snake_case + `nupa_` 前缀 |
| 编译器属性 | `objc_root_class`, `objc_direct` | `nupa_root_class`, `nupa_direct` | snake_case + `nupa_` 前缀 |
| 编译器内部符号 | `__block_impl`, `_cmd` | `__nupa_byref_X`, `_cmd` | `__` 前缀（编译器保留） |
| Ivar 私有变量 | `_name`（ObjC 属性合成） | `_name`（@synthesize） | `_` 单下划线前缀 |

---

## 2. 框架类（CamelCase + NP 前缀）

ObjC 的 `NS`/`CF`/`CG` 前缀 → Nupa 用 `NP`。

| 当前 | 规范 | 原因 |
|---|---|---|
| `NPObject` | ✅ 保持不变 | 同 `NSObject` |
| `NPClass` | ✅ 保持不变 | 运行时元类型，同 `objc_class` |
| `NPString` | ✅ 保持不变 | 同 `NSString` |

---

## 3. 运行时 C API（snake_case + nupa_ 前缀）

ObjC 用 `objc_` 前缀 → Nupa 用 `nupa_`。

| 当前 | 规范 | 原因 |
|---|---|---|
| `nupa_alloc` | ✅ | 同 `objc_alloc` |
| `nupa_init` | ✅ | 同 `objc_init` |
| `nupa_release` | ✅ | 同 `objc_release` |
| `nupa_retain` | ✅ | 同 `objc_retain` |
| `nupa_free` | ✅ | 裸机分配器 |
| `nupa_malloc` | ✅ | 裸机分配器 |
| `nupa_meta_init` | ✅ | 运行时初始化 |
| `nupa_isKindOf` | ❌ 应改为 `nupa_isKindOf`（首字母小写） | C 函数首字母小写，同 `objc_isKindOfClass` |
| `nupa_string_from_cstr` | ✅ | 同 `objc_string` 风格 |
| `nupa_weak_register` | ✅ | 同 `objc_weak_register` |
| `nupa_weak_unregister` | ✅ | 同 `objc_weak_unregister` |
| `nupa_weak_clear_all` | ✅ | 内部清理 |
| `nupa_weak_auto_cleanup` | ✅ | `cleanup` 函数 |
| `nupa_autoreleasepool_push` | ✅ | 同 `objc_autoreleasePoolPush` |
| `nupa_autoreleasepool_pop` | ✅ | 同 `objc_autoreleasePoolPop` |

> **注意**：`nupa_isKindOf` 中的 `K` 和 `O` 大写是因为 ObjC 中的 `isKindOf` 是 CamelCase 方法名转 C 函数。Nupa 的 C API 统一 snake_case，所以应改为 `nupa_is_kind_of`。

---

## 4. 运行时类型（snake_case + nupa_ 前缀）

ObjC 的 `objc_object`、`objc_class` 是 snake_case 结构体 → Nupa 同样。

| 当前 | 规范 | 原因 |
|---|---|---|
| `struct nupa_vtable` | ✅ | 统一 VTable 类型 |
| `enum nupa_vtable_index` | ✅ | VTable 索引枚举 |
| `struct nupa_X_meta_vtable` | ✅ | 元类 VTable |

---

## 5. 编译器属性（snake_case + nupa_ 前缀）

ObjC 的 `__attribute__((objc_root_class))` → Nupa 用 `nupa_` 前缀。

| 当前 | 规范 | 原因 |
|---|---|---|
| `__nupa_root`（类名） | ❌ 改为 `nupa_root` | `__` 前缀是编译器内部保留，不应作为类名 |
| `nupa___nupa_root_class` | ❌ 改为 `nupa_nupa_root_class` | 类名改了，元数据变量名跟随 |
| `@interface __nupa_root` | ❌ 改为 `@interface nupa_root` | 类声明 |
| `struct __nupa_root` | ❌ 改为 `struct nupa_root` | 结构体类型 |

**为什么 `__nupa_root` 要改？**

`__nupa_root` 是一个**实际存在的类**（有 struct 定义、有 `@interface`、有 `@implementation`），能被 `NPObject` 继承。`__` 前缀在 C 中保留给编译器实现，不应作为用户可见的符号。改成 `nupa_root` 后：
- 遵循 `nupa_` 前缀规则（同 `nupa_vtable`、`nupa_meta_init`）
- 不带 `__`，是公开的运行时类型
- 元数据变量 `nupa_nupa_root_class` 自然跟随

---

## 6. 编译器内部符号（`__` 前缀）

ObjC 的 `__block_impl`、`_cmd` → Nupa 用 `__nupa_` 前缀。

| 当前 | 规范 | 原因 |
|---|---|---|
| `__nupa_byref_X` | ✅ | 同 `__block_byref_X` |
| `__nupa_block_invoke_N` | ✅ | 同 `__block_invoke_N` |
| `__nupa_exception_buf` | ✅ | 异常跳转缓冲区 |
| `__nupa_exception_value` | ✅ | 异常对象存储 |
| `__nupa_saved` | ✅ | 嵌套 try 的 jmp_buf 保存 |
| `__nupa_state` | ✅ | try 状态变量 |
| `__nupa_tmp_N` | ✅ | 临时变量 |
| `__nupa_pool` | ✅ | 自动释放池变量 |
| `__nupa_sel_NAME` | ✅ | 选择器常量 |
| `_cmd` | ✅ | 同 ObjC 的 `_cmd` |
| `self` | ✅ | 同 ObjC 的 `self` |

---

## 7. Ivar 命名（`_` 单下划线前缀）

ObjC 属性合成 ivar 用 `_name` 前缀 → Nupa 相同。

```nupa
@property int age;
@synthesize age = _age;  // ivar 名为 _age
```

---

## 8. 不引入的 ObjC 特性

| ObjC 特性 | 不引入原因 |
|---|---|
| `objc_msgSend` | Nupa 使用 Uniform VTable 直接派发，不走 msgSend |
| `objc_getClass` / `objc_setClass` | Nupa 的类元数据通过 `nupa_X_class` 全局变量直接访问 |
| `objc_setAssociatedObject` | Nupa 不支持关联对象（无 Category 动态添加 ivar 需求） |
| Method Swizzling | Nupa 的 VTable 在编译期固定，不支持运行时替换 |
| `class_addMethod` | 同上，不支持运行时修改类 |
| `object_setIvar` | Nupa 的 ivar 布局在编译期固定 |
| `@dynamic`（已有） | 保留，用于分离 `@interface`/`@implementation` |
| `NSInvocation` | 过于动态，与 Nupa 的静态派发理念冲突 |
| `NSProxy` | 过于动态，暂不引入 |
| `respondsToSelector` | 暂不引入（VTable 索引固定，运行时无动态查找） |

---

## 9. 命名变更对照表

| 符号 | 旧名 | 新名 |
|---|---|---|
| 隐式根类名 | `__nupa_root` | `nupa_root` |
| 根类元数据变量 | `nupa___nupa_root_class` | `nupa_nupa_root_class` |
| 根类 @interface | `@interface __nupa_root` | `@interface nupa_root` |
| 根类 struct | `struct __nupa_root` | `struct nupa_root` |
| 根类 @implementation | `@implementation __nupa_root` | `@implementation nupa_root` |
| NSObject 父类声明 | `NPObject : __nupa_root` | `NPObject : nupa_root` |
| 类型判断函数 | `nupa_isKindOf` | `nupa_is_kind_of` |
| runtime.h 中 extern | `extern NPClass nupa___nupa_root_class` | `extern NPClass nupa_nupa_root_class` |

---

## 10. 代码生成命名模式

### 类元数据变量

```
nupa_{类名_flat}_class          → nupa_Person_class
nupa_{类名_flat}_vtable_inst    → nupa_Person_vtable_inst
nupa_{类名_flat}_meta_vtable    → nupa_Person_meta_vtable
nupa_{类名_flat}_meta_vtable_inst → nupa_Person_meta_vtable_inst
```

### 选择器常量

```
__nupa_sel_{selector名}         → __nupa_sel_init
                                → __nupa_sel_setName_
```

### Block 展开

```
__nupa_block_invoke_{N}         → __nupa_block_invoke_0
__nupa_block_layout_{N}         → __nupa_block_layout_0
__nupa_byref_{变量名}            → __nupa_byref_counter
```

### 临时变量

```
__nupa_tmp_{N}                  → __nupa_tmp_0
__nupa_pool                     → __nupa_pool
__nupa_state                    → __nupa_state
__nupa_saved                    → __nupa_saved
```