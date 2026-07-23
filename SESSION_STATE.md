# Nupa 编译器修复 Session 状态记录

**日期**: 2026-07-18 (周六)
**通过率**: 48/99 → 54/99 (总计 75/120)，无回归
**剩余失败**: 45 个，主要由分类N（泛型实例化收集）单点根因引起

---

## 已完成修复（17 项分类，#1-#17 除 #16 外全完成）

| # | 分类 | 根因 | 修复要点 | 涉及文件 |
|---|------|------|----------|----------|
| 1 | using_alias | `@using Alias` 未注册 type_name | `parse_using` 调 `add_type_name`，含 `@using NS::Class` short name | parser.rs |
| A | `@using` 泛型 | `parse_qualified_name` 不接受 `<...>` | 新增 `type_to_fqn`，`parse_using` 的 `Alias = FQN` 分支改用 `parse_type_full` | parser.rs |
| B | `@namespace`/`@using namespace` 限定名 | `match_name` 只取单 ident，遇 `::` 截止 | 改用 `parse_qualified_name_with_keywords` | parser.rs |
| C | msg send `<Type>` receiver | `parse_expression` 把 `<` 当比较运算符 | `parse_message_send` 先试 `parse_type_full` + lexer `save_pos`/`restore_pos` 重置 | parser.rs, lexer.rs |
| D | Block typedef 别名未注册 | namespace 内 typedef 被 stub 跳过 | `flatten_namespace_decls` 展开 + typedef 别名保留原名（非 mangled） | codegen.rs, elaborator.rs |
| E | 方法派发缺 `_cmd` | 用户直调 `NPObject_xxx(obj)` 缺 SEL | `FuncCall` 检测 `NPObject_<method>` 前缀自动补 `sel_registerName` | codegen.rs |
| F | ivar 命名/继承 | category 合成 ivar 覆盖主类 ivar | ivar 合并（非条件覆盖），两处 `class_infos.get_mut` | codegen.rs |
| G | `__block` 修饰丢失 | `convert_decl` 忽略 `is_block_qual` | 产出 `__block` 前缀 | codegen.rs |
| H | 非 ObjC 类成员误当 ivar | `PropRef` fallback 全产 `->` | `PropRef` 加 `is_arrow` 字段，按 `.`/`->` 区分 Member/Arrow | ast.rs, elaborator.rs, codegen.rs |
| I | parser 杂项 | typedef enum/数组 size identifier/`(weak)` ivar 属性 | `parse_typedef` enum 入口 + `array_size_name` 字段 + `(weak)` 括号消费 | parser.rs, ast.rs, cst.rs, elaborator.rs, codegen.rs |
| J | ivar 未识别为 ivar | superclass 未存入 symtab | binder 注册 superclass + struct 展开沿超类链前缀父类 ivar | binder.rs, codegen.rs |
| K | 方法派发缺函数定义 | （实为分类 J 范畴） | 同 J | - |
| L | 不兼容整数赋值 | `int**` 产成 `int *`（少一级 `*`） | `ast_type_to_c_str` 多级指针递归产出 | codegen.rs |
| M | diamond_impl 链接错误 | 多文件联编子文件 | 用户确认可忽略 | - |
| O | space_asteroid typedef enum | `parse_typedef` Enum 分支缺 `advance()` 消耗 `enum` + 无名 enum `{` body 分支 + 提前声明 | parser.rs (`parse_enum`/`parse_typedef`), codegen.rs (enum 提前声明) | parser.rs, codegen.rs |

### 剩余独立根因（未修，需独立 session）
- **`space_asteroid`** 剩 `_tv_sec`/`_tv_nsec`（分类 H 范畴的非 ObjC 类成员访问，`struct timespec` 字段访问被当 ivar）
- **`static_protocol_multi`** 字符串字面量含 `\"` 转义，lexer 字符串解析需重写

---

## 未完成：#16 分类N 泛型实例化收集（in_progress → pending）

### 根因
- 最小复现 `/tmp/min2.np`（`DataPack<QuantumToken*> alloc`）报 `undeclared identifier 'nupa_DataPack_QuantumToken_ptr_class'`
- codegen 没为 `DataPack<QuantumToken*>` 特化生成 class metadata 实例（struct/vtable/meta）
- 旧 C 版只有收集器 `gen_inst_add`/`collect_instantiations_decl`，没展开生成主体（也不完善）

### 已实施（未验证编译通过）
在 `crates/codegen/src/codegen.rs` 的 `ast_to_cg_unit` 顶层（`class_infos` 收集完后、`classes.push` 之前）插入约 200 行泛型实例化收集器：
- `collect_instantiations_expr` — 遍历 MsgSend receiver 收集 `Name<T*>` 字符串
- `parse_generic_type_string` — 解析 `Name<T1, T2*>` 渲染字符串为 `(base, type_args)`
- `split_top_commas` / `render_type_str_to_ast` — 辅助解析
- `walk_stmt_for_inst` / `walk_expr_for_inst` / `walk_decl_for_inst` — 递归遍历所有 decls/stmts/exprs
- 克隆泛型类的 `ClassInfo`，替换 `NPObject *`（TypePrim::Param 渲染）→ 具体类型字符串，插入 mangled flat 名

### 剩余编译错误（3 个，刚改完未验证）
1. `E0308`: `render_type_str_to_ast(a)` 传 `String` 需 `&str` — **已改** `render_type_str_to_ast(&a)`
2. `E0631`/`E0599`: `map(render_type_str_to_ast)` trait bounds — **已改** `map(|s| render_type_str_to_ast(s))` + 显式 `Vec<AstType>` 类型标注
3. 可能还有其他编译错误未暴露（cargo build 未跑完）

### 下一步（需独立 session）
1. `timeout 60 cargo build` 验证收集器代码编译通过，修剩余错误
2. `timeout 20 ./target/debug/nupac run /tmp/min2.np` 验证最小复现通过
3. 验证 `static_generics_template`/`zero_cost`、`ultimate_*`、`using_blockchain_regression`、`using_cyber_final_regression` 等用例
4. **重要**: 收集器只克隆了 `ClassInfo`（ivar/method 签名替换），但还需：
   - 克隆 method 函数定义（`unit.decls` 中的 `CgDeclData::Function`），替换方法体中的 `T` 类型
   - 生成特化 vtable struct + meta vtable struct + class metadata 实例
   - struct 展开时用特化 mangled 名（`struct DataPack_QuantumToken_ptr`）含 substituted ivar 类型
   - 这部分是数千行的大改动，单次对话易超时/挂后台

### 风险
- 收集器代码量约 200 行，未验证编译通过，可能还有 AST 枚举变体名/字段名不匹配的错误
- 特化展开主体（method cloning/vtable 重写）未实施，min2.np 即使收集器通过也不会完全通过

---

## 关键约束/偏好（本轮学到）
- **每条 bash 命令必须加 `timeout`**：用户电脑曾因我未加 timeout 的 `test_all.sh -j8` 挂后台进程导致 CPU 占用/温度持续，需重启
- `test_all.sh -j8` 并行 8 个 nupac+clang 编译，单条命令易超时/挂后台，慎用
- nupa 用编译期 vtable 静态派发，不是 `objc_msgSend`——不要引入 msgSend() 运行时派发
- 旧 C 项目 `../nupa-lang(old)/transpiler/src/` 可作参考，但也不完善（如泛型实例化只有收集器没展开生成）

## 关键文件修改清单
- `crates/parser/src/parser.rs` — parse_using/parse_namespace/parse_message_send/parse_typedef/parse_enum/数组 size/`(weak)` ivar
- `crates/codegen/src/codegen.rs` — name_flat 泛型 mangle/namespace flatten/FuncCall _cmd/ivar 合并/__block/PropRef is_arrow/多级指针/enum 提前声明/**泛型实例化收集器（未验证）**
- `crates/elaborator/src/elaborator.rs` — typedef 别名原名/convert_type array_size_name/PropRef is_arrow
- `crates/binder/src/binder.rs` — superclass 注册/ns_fqn
- `crates/ast/src/ast.rs` — AstType.array_size_name/PropRef is_arrow
- `crates/cst/src/cst.rs` — CstType.array_size_name
- `crates/lexer/src/lexer.rs` — save_pos/restore_pos

## 最小复现文件
`/tmp/min2.np`（重启后已删，需重建）:
```nupa
#import <Foundation/Foundation.nh>
@interface QuantumToken : NPObject
@end
@implementation QuantumToken
@end
@interface DataPack<T> : NPObject {
    @public
    int _count;
}
@end
@implementation DataPack
@end
int main() {
    @autoreleasepool {
        DataPack<QuantumToken*> *p = [[DataPack<QuantumToken*> alloc] init];
        (void)p;
    }
    return 0;
}
```
预期错误: `undeclared identifier 'nupa_DataPack_QuantumToken_ptr_class'`
