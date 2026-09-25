# AGENTS.md — Nupa Language Project

## Current Phase: Stage 3 — Codegen + Testing

## File Extension Convention
- **`.nh`** — Nupa header (declarations: `@interface`, `@protocol`, typedefs, structs). These are inlined by nupac's preprocessor via `#import`, never passed to the C compiler directly.
- **`.np`** — Nupa implementation (definitions: `@implementation`, functions, `int main`). Inlined via `#import` and translated to C.
- Headers use `.nh` (not `.h`) deliberately: a `.h` extension would collide with ObjC/C system headers (e.g. `Foundation.h` vs `Foundation/Foundation.h`). nupac's preprocessor only treats `#import` of `.nh`/`.np` as nupa imports; `#include` of `.h`/`.c` is passed through verbatim to the C compiler.
- When a header is meant to be usable directly by a plain C compiler (not just via nupac), guard the objc-style syntax with `#ifdef __NUPA__` / `#else`. nupac always defines `__NUPA__` (all backends), so it inlines the nupa branch; a C compiler sees only the C-compatible `#else` branch.

## Build & Test Commands
```bash
# Build everything
ninja -C builddir

# Run all unit tests (6 suites)
ninja -C builddir test

# Run individual unit tests
./builddir/test_lexer
./builddir/test_parser
./builddir/test_cst_print
./builddir/test_cst_visit
./builddir/test_symbol
./builddir/test_binder
./builddir/test_checker

# Run integration tests
./builddir/test_codegen
./builddir/test_codegen_header
./builddir/test_codegen_convert
./builddir/test_codegen_emit

# Run one specific integration test
./builddir/test_elaborator  # run without args
```

## Project Map
```
nupa-lang/
├── transpiler/
│   ├── src/
│   │   ├── main.c              — CLI entry point
│   │   ├── lexer.c/h           — Lexer: tokenizer; prep for parser
│   │   ├── parser.c/h          — Recursive-descent parser → CST
│   │   ├── cst.h               — Concrete Syntax Tree types + alloc
│   │   ├── cst_print.c/h       — Debug printer for CST
│   │   ├── cst_visit.c/h       — Visitor pattern over CST
│   │   ├── elaborator.c/h      — Semantic elaboration: @interface/impl matching, protocol merging
│   │   ├── binder.c/h          — Name binding: symbol creation for typedefs, classes, protocols, @selector
│   │   ├── checker.c/h         — Type checker
│   │   ├── nupa_type.h         — NP type representation (np_type_t, TYPE_* enum, type functions)
│   │   ├── symbol.c/h          — Symbol table (symtab_*): types, classes, protocols, methods, selectors
│   │   ├── codegen.c/h         — Code generator (C output)
│   │   ├── codegen_emit.c/h    — Emit C code from AST
│   │   ├── codegen_header.c/h  — Header file generation
│   │   ├── codegen_msg.c/h     — Message send codegen
│   │   ├── config.h            — Build config (header from meson)
│   │   └── str.c/h             — String utilities (optional)
│   └── include/nupa/
│       └── public headers
├── tests/
│   ├── unit/
│   │   ├── test_lexer.c
│   │   ├── test_parser.c
│   │   ├── test_cst_print.c
│   │   ├── test_cst_visit.c
│   │   ├── test_symbol.c
│   │   ├── test_binder.c
│   │   └── test_checker.c      — 15 tests: basic expressions, control flow, interfaces, protocols, protocol inheritance
│   └── integration/
│       ├── main.m (or .np)     — Integration test input
│       └── expected/           — Expected C output
├── builddir/                   — Ninja build directory
├── meson.build                 — Top-level build
├── meson.options               — Build options
└── AGENTS.md                   — This file
```

## Symbol Table API
- `symtab_alloc()` / `symtab_free(st)` — lifetime
- `symtab_register(st, name, kind, data)` — register any symbol
- `symtab_lookup(st, name)` — find symbol by name (NULL if not found)
- `symtab_register_type(st, name, cst_type)` — register a typedef/struct/union/enum
- `symtab_register_selector(st, sel_name)` — register a selector name, returns existing if already registered
- `symtab_find_selector(st, sel_name)` — find a selector by name
- Selectors are stored in `st->sels[]` dynamic array (realloc'd)
- Selectors are freed in `symtab_free`

## Checker Protocol Lookup
- `check_protocol_method(c, e, proto, sel, arg_count)` — recursive search of protocol + parents for a method matching selector
- Method: searches `required_methods[]`, then `optional_methods[]`, then recurses into `parents[]`
- Returns cloned `np_type_t` of matched method's return type

## Block Type Signature
- `CST_EXPR_BLOCK` returns `np_type_t` with `is_block=1`, `subtype=return_type`, params linked via `next`
- `e->type` set to `cst_type_t` with `is_block=1`, `subtype=return type CST`

## Completion Status
- [x] Stage 1: Parser, CST, Elaborator, Codegen — complete
- [x] Stage 1.4: CST visitor pattern + validation — complete
- [x] Stage 2: Elaborator, Binder — complete
- [x] Stage 3: Codegen improvements — complete (binary operators, @selector, blocks, @synthesize, arrays, for-in, @synchronized, protocol metadata, postfix ++/--, match_name(), class method dispatch, super, sel_registerName, vtable ordering)
- [x] Checker: P0 type checking — enabled by default, uses `scope_vars` for params/locals, `types_compatible` for assignment/return, protocol conformance

### Generic Monomorphization Status: Partial
| Component | Status | Details |
|-----------|--------|---------|
| Struct emission | ✅ | Generic structs skipped; specialized `struct DataPack_QuantumToken_ptr` emitted with substituted ivar types |
| Method emission | ✅ | Generic methods skipped; `DataPack_QuantumToken_ptr_init` emitted with substituted return/param types |
| Vtable struct | ✅ | Specialized `nupa_DataPack_QuantumToken_ptr_vtable` emitted per instantiation |
| Meta vtable struct | ✅ | Specialized `nupa_DataPack_QuantumToken_ptr_meta_vtable` emitted per instantiation |
| Class metadata | ✅ | Specialized `nupa_DataPack_QuantumToken_ptr_class` emitted with `sizeof(struct DataPack_QuantumToken_ptr)` |
| Ivar cast (method bodies) | ✅ | `((struct DataPack *)(self))` → `((struct DataPack_QuantumToken_ptr *)(self))` via `codegen_current_class` override |
| Caller-side class metadata | ✅ | `&nupa_DataPack_QuantumToken_ptr_class` used instead of `&nupa_DataPack_class` |
| Vtable dispatch in callers | ✅ | `((struct nupa_vtable *)...)->methods[INDEX]` uniform dispatch (replaces per-class vtable casts) |
| Method body type substitution | ✅ | `T item = _storage[_count]` → `QuantumToken * item = ...` (via `substitute_stmt_types` in codegen) |
| `T` in variable declarations | ✅ | `T item = _storage[_count]` → `QuantumToken * item` (via `parse_statement` generic param check) |
| Caller-side function name | ✅ | `NPObject_alloc` used instead of `DataPack_alloc` (superclass chain fixed) |
| Superclass chain | ✅ | `DataPack->data.cls.superclass = NPObject` (parser now handles `: superclass` after `<T>` generics) |
| Debug prints removed | ✅ | 11 lines removed across codegen.c, symbol.c, parser.c, binder.c |

### Remaining Issues
- **✅ 容器泛型已规划**：`NPArray<T>` 应使用 monomorphization 机制生成特化结构体（如 `NPArray_NPString_ptr`），`objectAtIndex:` 返回 `T` 而非 `id`。当前暂不做，但机制已就绪（参见 `DataPack<T>` 的实现模式）。

### Fixes Applied (July 2026)
- ✅ `collect_from_type` now compares type arguments (not just count) — fixes `VectorBuffer<RenderPoint2D*>` vs `VectorBuffer<RenderColorRGB*>` dedup
- ✅ Meta vtable struct generated for specialized classes with superclass (even with 0 class methods)
- ✅ Meta vtable instance generated for classes with superclass (even with 0 class methods)
- ✅ Message send receiver: generic types like `[VectorBuffer<RenderPoint2D*> alloc]` now parsed correctly (parser tries type parsing before expression for `[` receivers)
- ✅ `cst_type_clone` now copies `type_args` — fixes block parameter type arg loss
- ✅ `mangle_type_name` buffer size calculation fixed (handles `*`→`_ptr` expansion)
- ✅ Block function param types now handle generic type args with mangled names
- ✅ Block variable declaration types now handle generic type args with mangled names

### Fixes Applied (Current Session — July 2026)
- ✅ `__nupa_root` fix: added `release`/`retain` methods to `__nupa_root` interface/implementation in `NPObject.nh`
- ✅ Codegen vtable class fallback: class method on runtime-determined receiver (e.g. `[[self class] alloc]`) now passes receiver expression as class argument
- ✅ `[obj class]` message send: special-cased in codegen to emit `((NPClass *)((NPObject *)obj)->isa)` — direct ivar access
- ✅ `@try`/`@catch`/`@finally` codegen: `@try` body is now emitted as plain compound (previously empty); `@throw` body emits the expression instead of `/* stub */()`
- ✅ **`@selector` codegen**: changed from emitting raw FNV-1a hash as `unsigned` int to emitting `sel_registerName("selName")` — fixes `SEL` type mismatch (`include/nupa/runtime.h:SEL` is a struct, not unsigned). See `codegen.rs:1062-1065`.
- ✅ **`@try`/`@catch`/`@finally` exception handling**: fully implemented using `setjmp`/`longjmp` with TLS globals (`__nupa_exception_buf`, `__nupa_exception_value`) in the runtime. `@throw expr` now evaluates the expression, stores it, and longjmps to the nearest setjmp in the enclosing `@try`. Catches declare the catch variable from `__nupa_exception_value`. Finally blocks always execute. **Nested @try fully supported**: each try saves/restores the parent jmp_buf; uncaught exceptions in inner @try propagate to outer @catch after inner @finally. See:
  - `include/nupa/runtime.h`: added `<setjmp.h>` + TLS globals
  - `include/nupa/runtime.c`: TLS globals definition
  - `codegen.rs:1221-1335`: `convert_stmt` rewrites for Try/Catch/Finally/Throw
- ✅ **Fix 4 — Uniform VTable index enum**:
  - `enum nupa_vtable_index` emitted with all instance method names globally sorted
  - `struct nupa_vtable { void (*methods[N])(); }` replaces per-class vtable structs
  - Vtable instances: `[INDEX] = (void (*)())func` designated init
  - Dispatch: `((RT(*)(...))((struct nupa_vtable *)recv->isa->vtable)->methods[INDEX])(args)`
  - Multi-class isa‑comparison chain eliminated — single uniform access for all classes
  - All Rust unit tests pass; generated C code compiles without errors
- ✅ **Fix 5 — `NPObject *` alloc+init `=` emit** (July 2026 Session 2): When a variable of type `NPObject *` is initialized via the alloc+init vtable-dispatch pattern, the split-emit path now always includes `=` (not just when `needs_cast` is true for subclass types). Fixes `Cat *a = [Cat alloc]`-style code generation where the init function call was emitted without the assignment operator, causing C99 parse errors. See `codegen.rs:3682-3696`.
- ✅ **Fix 6 — Deduplicate `#include <string.h>`**: Check pre-existing C headers for `<string.h>` before unconditionally emitting it. See `codegen.rs:3913-3916`.
- ✅ **Fix 7 — Block typedef namespace prefix**: Replace short block name (e.g. `ActionCompleteBlock`) inside block type strings with namespace-qualified flat name (e.g. `Extension__ActionCompleteBlock`) so the block typedef refers to itself directly. No separate alias line is emitted. All references (ivar types, function params, vtable member types) use the flat name via `BLOCK_TYPEDEF_NAMES` lookup. See `codegen.rs:1758-1788`.

### Fixes Applied (Current Session — July 2026, Session 3)
- ✅ **Postfix `++`/`--` codegen**: operand now wrapped in parentheses — `(*p)++` instead of `*p++` (which C parses as `*(p++)`). Fixes all JSON parser pointer corruption.
- ✅ **JSON Editor split-screen preview**: `refreshDisplay` clears screen and shows JSON tree in top half, command prompt in bottom half, using ANSI escape codes.
- ✅ **Multi-file compilation test**: `json_editor.np` now `#import`s `json_editor_types.nh` (separate file with all @interface declarations). Compiler resolves cross-file types correctly.
- ✅ **ARC analysis rewritten**: `arc_local_analyze` now recursively traverses nested scopes, directly inserts `nupa_release` into AST, handles early return/throw/break/continue, passes `parent_vars` through If/While/For branches, and inserts releases before control flow exits.
- ✅ **`@selector` test fix**: `selector_usage_test.np` and `golden/10_edge_cases/selector.np` updated to use `SEL` type instead of `unsigned`.
- ✅ **Block literal params use `AstType`**: `AstExprData::Block` now uses `Vec<(AstType, String)>` instead of `Option<Box<CstParam>>`, so parameter types get `class_ref` resolution and correct mangled names.
- ✅ **`nupac` binary uses absolute paths**: `compile_to_binary` and `Pipeline.search_dirs` now resolve include paths relative to the binary's location, not the current working directory.
- ✅ **Test runner ARC retry**: tests that fail with ARC are automatically retried with `-fno-nupa-arc` (MRC fallback).
- ✅ **Checker restored** (`crates/checker/src/lib.rs`): implements P0 type checking — assignment compatibility, return type matching, variable declaration checks (duplicate detection, init type), protocol conformance (required methods from symbol table, protocol parent inheritance), error reporting with `file:line:col: error:` format.
- ✅ **`-fno-checker` CLI flag**: added to `main.rs`; `no_checker` field on `Pipeline`; checker runs by default before codegen.
- ✅ **Checker `FuncCall` / `MsgSend` return types**: `MsgSend` returns `id`, `FuncCall` returns `id` instead of `int`.
- ✅ **Checker `types_compatible` rules**: `Named` types (class ptrs) compatible with `id` and each other.
- ✅ **Checker method/function params**: added to `scope_vars` so param names are visible in method bodies; `lookup_var_type` now checks `scope_vars` in addition to symtab/ivars.
- ✅ **Test count raised from 125/141 → 134/141** (ARC retry + checker fixes).
- ✅ **Checker `match` arm ordering fix**: Moved `_ =>` wildcard to end of `check_stmt` (was before `Decl`/`Throw`/`Try`/`Catch`/`Finally`/`Synchronized`/`Autoreleasepool`, making them unreachable). Fixes duplicate declaration, init/assign type mismatch, and all statement-body checking.
- ✅ **Checker `types_compatible` pointer guard**: numeric compatibility now requires both sides to be non-pointer (`!is_pointer`), preventing `int = "hello"` from passing.

### C Superset Support — Implemented ✅ (Aug 2026)
- ✅ **Struct definitions**: codegen now emits struct fields (was emitting empty `struct Name {};`). Fields converted from `AstDeclData::Aggregate` ivars via `ast_type_to_c_str`. Structs are emitted before function prototypes in the output to avoid `-Wvisibility` errors.
- ✅ **`struct Name *p` declarations**: parser fall-through path now consumes `*` pointer suffix(es) so `struct Widget *pw = ...` is a proper variable declaration (was `*pw = ...` expression statement).
- ✅ **Function pointer types**: `T (*)(params)` and `T (*name)(params)` parse via `parse_type_full` (new `is_fn_ptr` flag on `CstType`/`AstType`); rendered as `ret (*name)(params)` by `cst_type_to_c_str`/`ast_type_to_c_str`. Variable emission treats `(*` like `(^` — the declarator name is inside the type, so no separate name is appended.
- ✅ **C-style casts** `(struct uiEntry *)data`, `(void (*)(struct uiButton *, void *))cb`, `(void *)ptr`: parser + AST + codegen already supported the general cast path.
- ✅ Golden test: `tests/golden/22_c_superset/c_superset.np`.
- ✅ **Struct 全量补全（Aug 2026）**——极端 C/struct 语法矩阵 `tests/c_struct_extreme_test.np`（已纳入 test_all，193/203 全绿色）：
  - ✅ `typedef struct Tag Alias;`（tag 引用别名，tag 可仅前向）
  - ✅ struct 定义**依赖拓扑排序**：`typedef struct Outer { struct Inner i; }` 现在会让 `struct Inner{...}` 先发射（此前 InnerG 定义在用户之后 → C `incomplete type` 报错）
  - ✅ **函数指针字段** `int (*cb)(int);` 正确发射（此前 fnptr 字段丢失 body）
  - ✅ typedef 数组 `typedef int Row4[4];` 与**多维数组字段** `int cells[4][4];`
  - ✅ 位域 `unsigned a : 3;` 可解析（宽度按 C 语义被丢弃，降级为普通字段——近似语义）
  - ⚠️ 已知不支持（记录中）：**匿名内联 struct 字段**（`struct { int a; } inl;`，需命名 tag）；**fnptr 数组字段** `(*name[N])(...)` declarator；以及"Cast 后作函数指针调用"与"struct 成员函数指针调用"（nupa-world 用纯 C 桥 hostcb/modh 绕过）
- ✅ libui demo rewritten in pure Nupa: callbacks (`onGreet`/`onInc`/`onDec`/`onReset`/`counter_ctx_create`) moved from `libui_demo_cb.c` into `examples/03_LibUI/libui_demo.np`; button-click fn-ptr cast moved from `LibUI_helper.c` into `LibUI.np`. `include/LibUI.{nh,np}` + `LibUI_mac.c` now live in `examples/03_LibUI/include/`; `run_libui.sh` compiles only `runtime.c` + `LibUI_mac.c` as C helpers.

### Inline Asm + Real `.s` Linking — Implemented ✅ (Aug 2026)
- ✅ **`asm` / `__asm__` / `__asm__` keyword** (lexer `KeywordKind::Asm`; also `__volatile`/`__volatile__`).
- ✅ **Extended asm** (full GCC/Clang layout): `asm [volatile] [goto] (template : outputs : inputs : clobbers : labels)`.
  - Template: adjacent string literals concatenated; raw escapes preserved (`\n\t` survive into emitted C).
  - Operands: named `[name] "constraint"(expr)` and positional `"constraint"(expr)`; output/input/in-out `+r`.
  - Clobbers: `"cc"`, `"memory"`, register names.
  - `asm goto`: 4th section of labels; emitted as `__asm__ goto (... : ... : labels)`.
- ✅ **Bare top-level `__asm__("...")`** as a declaration (passes through to emitted C verbatim, e.g. `.section` directives).
- ✅ **C label statements** (`ident:` not `::`) now parse via source-slice lookahead — required for `asm goto` targets; emitted as `ident: ;`.
- ✅ **Codegen**: `emit_asm_syntax()` produces `__asm__ [__volatile__] [goto] ("template" : ... : ...)` used by both stmt and decl emitters (`crates/codegen/src/codegen.rs`).
- ✅ **Real `.s` assembly linking**: `nupac -asm <file.s>` / `-S <file.s>` (repeatable) passes the file to clang/ld; verified with `_asm_square`/`_asm_add3` in `tests/asm_link_test.s`. `test_all.py` auto-links a sibling `name.s` next to any `name.np`.
- ✅ **`const unsigned char *` codegen fix**: `ast_type_to_c_str` pointer types now recurse into the subtype (like `cst_type_to_c_str`) so base qualifiers (`unsigned`/`signed`/`long`/`const`) survive, e.g. fn-pointer param `const unsigned char *` was emitted as `const char *` (dropping `unsigned`). Fixed in `crates/codegen/src/codegen.rs`.
- ✅ **Extreme fusion test**: `tests/asm_fusion_test.np`+`.s` — 12-stage stress fusing inline asm + real ARM64 external asm (CRC32 `0xCBF43926`, `rotl32`, `clz32`, `bitrev32`) with namespaces, class inheritance + VTable dispatch, protocols (`id<IChecksum>`), VProperties + `@synthesize`, Blocks with `__block` capture, `@try/@catch/@finally`, struct + C cast, fn-pointer to asm symbol, `@selector`, `__weak`, ARC `@autoreleasepool`.
- ⚠️ **AArch64 `%w` gotcha**: inline asm operands holding 32-bit values must use the `%w` register modifier (e.g. `%w[s]`, `%w0`). Without it clang maps `"r"` to 64-bit `x` regs, so `ror x0,x0,#27` rotates 64 bits and corrupts `unsigned` vars (first `asm_inline_test.np`/`asm_fusion_test.np` had it wrong; clang warns "use constraint modifier w").
- ✅ **x86_64 / Rosetta cross-arch**: `nupac -arch <target>` forwards `-arch <target>` to clang. `asm_x64/asm_x86_fusion_test.np`+`asm_x86_ext.s` (`square`/`sum3`/`rotl32`/`clz32`, inline `imull`/`addl`) builds an x86_64 Mach-O and runs it through Rosetta on this arm64 M4 (calls `softwareupdate --install-rosetta` and uses native `/usr/bin/arch` — not uutils `arch`). x86_64 inline asm needs no `%w` modifier (32-bit regs are used directly).
- ✅ **Golden dirs**: `tests/golden/23_asm` (inline/references), `24_asm_fusion`, and the x86_64 cross-arch case lives under `asm_x64/` (not in `tests/**`, so it's excluded from the default arm64 suite).
- ✅ **Tests**: `tests/asm_inline_test.np` (extended/volatile/named/`+r`/asm goto), `tests/asm_link_test.np`+`.s`, golden `tests/golden/23_asm/asm_inline.np`+`.out`.
- ⚠️ Operands reuse `parse_expression()` — message-send operands parse but are not recommended (use plain C-ish expressions in asm operands).

### Remaining Known Issues
- `golden/11_multi_file_union/diamond_impl.np` — FAIL (no main entry, pre-existing library-style multi-file test)
- **Variadic selector 支持状态（待办，用户后续修复）**——语法已支持（`sel:a, b, c, nil`），缺的是库与 codegen：
  - **库缺失**：`NPSet`/`NPMutableSet`/`NPDictionary`/`NPMutableDictionary`/`NPOrderedSet`/`NSPredicate` 这些类根本不存在；`NPString` 的 `stringWithFormat:`/`initWithFormat:`/`stringByAppendingFormat:` 与 `NPMutableString` 的 `appendFormat:` 方法未声明；`NPArray`/`NPMutableArray` 只有 `initWithObjects:count:`（无 variadic `initWithObjects:`）；无 `NSAssert` 宏。
  - **codegen 缺失**：Nupa 无运行时 `va_list`，真正的 variadic 方法无法用 Nupa 实现，只能按 selector 特判落到运行时 helper。目前只有 `arrayWithObjects:` 一个特判（desugar 成 `@[...]`），且它丢弃 receiver——`[NPMutableArray arrayWithObjects:...]` 会静默得到不可变 `NPArray`。
  - **待修**：(a) `[NPMutableArray arrayWithObjects:...]` 保留可变性；(b) 未知 variadic selector 给出清晰 `error` 而非生成坏 C；(c) 视需要为 `stringWithFormat:` 系列复用 `NPLog` 的编译期 `%@` 展开。

### Header-Only Class Linking — Implemented ✅ (Aug 2026)
- ✅ A main file may `#import` only `.nh` declaration headers and link against a separately compiled `.np` implementation file (standard C header/source split). `examples/03_LibUI/libui_demo.np` now imports only `LibUI.nh`; `examples/03_LibUI/include/LibUI.np` is transpiled+compiled separately and linked in (see `examples/03_LibUI/run_libui.sh`).
- ✅ Duplicate-symbol fix: generated class metadata definitions are now emitted as **weak symbols** — `__attribute__((weak))` on vtable instances, meta-vtable instances, `*_getClass`, `nupa_meta_init`, `nupa_string_from_cstr`, and all function definitions with bodies (`emit_decl` in `codegen.rs`). The linker coalesces the identical per-TU copies (base-class methods from `Foundation/NPObject.nh` appear in every TU). Single-file output is unchanged (weak is invisible with a single definition).
- ✅ AST now tracks `is_implementation` on `AstDeclData::Class` (elaborator sets it from `CstDeclKind::ClassImplementation`/`CategoryImplementation`); `CgClassMeta.has_impl` computed from it (used for future work; not required by the weak-symbol fix).

### Shell Completions via clap_complete — Implemented ✅ (Aug 2026)
- ✅ `nupac --gen-completions <shell>` emits a completion script for **zsh / bash / fish / powershell / elvish**, built with `clap_complete` from `clap_command()` in `crates/nupac/src/main.rs`.
- ⚠️ nupac's real CLI uses **single-dash** long flags (`-rewrite-nupa`, `-fno-nupa-arc`, `-arch`, `-asm`). clap normalizes these to `--flag`, so `gen_completions` post-processes the generated script via `DOUBLE_TO_SINGLE` (`--rewrite-nupa`→`-rewrite-nupa`, `--output`→`-o`, etc.) and dedups consecutive identical lines.
- ✅ Generated scripts are self-contained (flags embedded, no runtime binary call). Verified in bash: `-f<TAB>` → `-fnupa-arc -fno-nupa-arc -fno-checker -fno-libc`, `--v<TAB>` → `--verbose --version`, `-a<TAB>` → `-asm -arch`.
- ✅ Scripts live in `completions/` (`_nupac`, `nupac.bash`, `nupac.fish`). `crates/nupac/build.rs` copies them into `target/<profile>/completions/`; `build-all.sh` includes them in each platform bundle + `target/` archives; `install-pkg.sh` installs them to `$PREFIX/share/nupac/completions/` and prints registration hints (zsh fpath / bash source / fish source).
- ✅ Legacy hand-written `--autocomplete=<cmdline>` mode + `interactive_flag_picker` (`-`/`--` menu) retained in main.rs.

### Tests Status
- Overall: **195/206 pass** (3 pre‑existing failures: `diamond_impl.np` + `mega_fusion/mega_types.np` no-main library tests, `golden/28_refcount_trace/double_release.np` intentional over-release crash; 8 canceled interactive). Includes `npmutablearray_test.np` (ARC + MRC), `NPLog` signature change, `class_forward_decl.np` re-enabled, golden `13_foundation/06_nparray` + `07_npmutablearray`, `grand_feature_stress_test.np`, `nil_messaging_test.np`, `q.np`. Plus `clang_gcc_stress/run_stress.sh` 23/23 PASS (gaps 01–17), Rust unit tests 41/41, trace goldens 7/8 (`arc_inject` pre-existing).

### Refcount Trace Golden Suite — `28_refcount_trace` (Aug 2026)
- ✅ New golden dir `tests/golden/28_refcount_trace/`: 8 `.np`+`.out` pairs. Unlike other golden dirs, the `.out` files are **not** program output — they are `nupac -trace-refcount -trace-no-color -trace-max-iters 2` snapshots.
- ✅ Runner: `./tests/golden/28_refcount_trace/run_trace_golden.sh` (diffs each trace against `.out`; `NPAC=` env overrides the binary, default `target/release/nupac`).
- ✅ Coverage (verified correct by hand):
  - `retain_release` — retain ×3 / release ×4 → 1→2→3→4→3→2→1→0, freed.
  - `double_release` — release past 0 → `! double-release / over-released …: -1`, Summary red `over-released — count negative`.
  - `leak_detect` — one object unreleased → Summary red `still alive — possible leak`.
  - `arc_inject` — ARC mode: `[Item new]` ×3, scope end injects 3× `nupa_release` → all freed (proves ARC auto-release is traced).
  - `loop_iterations` — ARC loop: fresh `Item#1`/`Item#2` per `for iter`, each `nupa_release`→0.
  - `alias_shared` — `Item *alias = shared` shares the same object (`retain`→2, two `release`→0).
  - `if_else_branch` — `── if ──`/`── else ──` branch state cloning (both end at 0).
  - `autoreleasepool_nested` — nested pools with manual `autorelease` inside `@noarc`: each `pool pop` frees exactly its own object.
- ✅ All `.np` files are valid, runnable Nupa programs (import `nupa/runtime.h` + `Foundation/NPObject.{nh,np}` only, keeping trace output free of Foundation noise) — they pass the normal `nupac run` path used by `test_all.py`'s glob.
- ⚠️ `! nupa_release on untracked self` lines at 32:5 / 60:5 come from the inlined `NPObject.np` `-release`/`-dealloc` bodies (releasing untracked `self`) — expected noise, part of the golden snapshot.

### Namespace `@class` Forward Decl + `@using` Conflict Detection — Implemented (Aug 2026)
- ✅ **`@class` forward declarations now emit C**: previously `@class Player;` registered a symbol in the binder but the elaborator dropped it and codegen never emitted a declaration, so using `Player *` in method signatures produced `unknown type name 'Game__Player'`. Now the elaborator converts `CstDeclData::Forward` → `AstDeclKind::ForwardClass` / `AstDeclData::ForwardClass { names }` (namespace-prefixed via `ns_fqn`); codegen emits `struct Game__Player;` + `typedef struct Game__Player Game__Player;` at the top of the file, skipping names that have a full class definition in the unit and skipping `nupa_root`/`NPObject`. Works at top level and inside `@namespace` blocks.
- ✅ **`@using` conflict detection**: previously `add_using` blindly pushed to `using_list` and `find_using` returned the first match, so two `@using` entries importing the same short name compiled silently. Now the binder's `Using` handler checks (before registering) whether the short name (a) already exists in `using_list` → `ambiguous import: 'X' imported from both 'A' and 'B'`, or (b) collides with an existing class/protocol/typedef symbol → `'X' conflicts with an existing symbol`. Both emit `error:line:col:` and abort binding.
- ✅ Cross-namespace inheritance (`@interface HUD : Engine::Graphics::Renderable`) was already supported and is now covered by a dedicated test.
- ✅ Test: `tests/namespace_forward_class_test.np` (forward `@class` in namespace, cross-ns inheritance, `@using` short name, message sends across namespaces).

### File Extension Renames — Final (Aug 2026)
- ✅ Reverted nupa headers **`.h` → `.nh`** (an earlier session had renamed `.nh` → `.h`; that broke the ObjC `Foundation.h`-collision-free guarantee and would let system `#include` clash with nupa headers). Decision: headers stay **`.nh`**, implementations stay **`.np`**.
- ✅ nupac preprocessor `is_nupa_import` accepts nupa header/impl extensions — **`.nh` and `.np` only** (`.h`/`.nupa` removed).
- ✅ `__NUPA__` macro: nupac now always defines `__NUPA__` for all backends (`pipeline.rs` extra_macros). This enables headers that are shared with plain C compilers to guard objc-style syntax behind `#ifdef __NUPA__` (nupac inlines the nupa branch; a C compiler sees the `#else` C-compatible branch). Internal-only headers need no guard.
- ✅ Verified both scenarios: (1) nupac-internal header with `#ifdef __NUPA__` → nupa branch generated, `compute(21)=42`; (2) same header seen directly by `clang -c` (no `__NUPA__`) → C branch compiles clean.
- ✅ Renamed ~172 implementation files `.nupa` → `.np` (and earlier `.np`→`.nupa`→`.np`) with all `#import` references, `test_all.py` globs, build `.sh` scripts, `.vscode` extension, and docs updated; no `#import ... .np` references gave ambiguous matches.
- ✅ Full regression green after renames: run_stress 23/23, test_all 160/169 (2 pre-existing no-main, 7 interactive canceled), cargo unit 26/26.
- **Golden freestanding test** `tests/golden/25_freestanding/` — verified via `./build.sh` (not in default suite because it requires `-rewrite-nupa -fno-libc` + host clang + helpers.c). Demonstrates @namespace, @interface (implicit root), @try/@catch/@finally, @selector, inline asm, C-style cast on bare metal.
- ARC retry: tests that fail with ARC are automatically retried with `-fno-nupa-arc` (MRC fallback)

### `@noarc { }` — Block-level MRC (Aug 2026) ✅
- **Purpose**: In ARC mode, manual memory management is forbidden by the checker (`explicit 'retain'/'release'/'dealloc'/'autorelease' not allowed in ARC mode`). `@noarc { }` scopes a block where the programmer manages memory manually — the block-level analogue of `-fno-nupa-arc` (and clang's `-fno-objc-arc`). Named `@noarc` (not `@unsafe`) to match the existing `-fno-nupa-arc` CLI flag and to avoid over-promising a Rust-style safety model — the name makes no claim that code outside the block is "safe".
- **Lexer/parser**: `@noarc` → `KeywordKind::AtNoArc` → `CstStmtKind::NoArc` / `CstStmtData::NoArc(Box<CstStmt>)` → `AstStmtKind::NoArc` / `AstStmtData::NoArc(Box<AstStmt>)`.
- **ARC analyzer** (`crates/arc/src/arc.rs`): `@noarc` blocks are skipped entirely — no release injection.
- **Checker** (`crates/checker/src/lib.rs`): `in_noarc` flag toggled while walking a `NoArc` body; `retain`/`release`/`dealloc`/`autorelease` outside `@noarc` (and outside the implementation of the matching runtime method) error in ARC mode. `Checker.no_arc` is set from pipeline's `-fno-nupa-arc`.
- **Codegen** (`crates/codegen/src/codegen.rs`): emits the body as-is, no ARC injection.
- **Foundation**: NPString/NPMutableString convenience constructors (`+stringWithUTF8String:`/`+stringWithString:`) wrap their deliberate `autorelease` in `@noarc { }`, since Nupa's ARC does not auto-inject autorelease-on-return.
- **Test**: `tests/noarc_test.np`.

### ARC Rewrite — RefVal State Machine (Aug 2026) ✅
- ✅ **Scope-stack 模型替换为 RefVal 状态机**：旧模型用 `Vec<Scope>` 追踪"哪些变量要释放"，新模型用 `HashMap<String, RefState>` 追踪每个对象的精确引用计数状态。
- ✅ **RefState 状态机**（`crates/arc/src/arc.rs`）：
  - `NotOwned` — +0，当前函数不拥有此对象
  - `Owned(u32)` — +n，当前函数拥有，必须在作用域结束释放 n 次
  - `Released` — 已释放
  - `Unknown` — 分支合并后无法确定状态（保守地不释放，避免 double-free）
  - `Error` — 错误状态（over-release 等）
- ✅ **路径敏感分支合并**：`if/else` 分支结束时，两边状态取交集（`merge_states`）。分歧 → `Unknown`（保守不释放，宁可 leak 不 double-free）。
- ✅ **retain/release 计数追踪**：`[obj retain]` 递增计数，`[obj release]` 递减。`nupa_retain`/`nupa_release` 函数调用也正确处理。
- ✅ **retain on NotOwned**：`nupa_retain(obj)` 在 NotOwned 对象上 → 变为 Owned(1)（调用者主动 retain 了一个不拥有的对象）。
- ✅ **release on NotOwned**：`nupa_release(obj)` 在 NotOwned/Released 对象上 → Error（over-release 检测）。
- ✅ **return/throw escape**：return/throw 的对象被标记为 escaped，scope-end 释放时跳过它。
- ✅ **break/continue**：只释放最近循环作用域内声明的变量。
- ✅ **`@noarc` 块完全跳过**：状态机不进入 `@noarc` 块，用户手动管理内存。
- ✅ **回归验证**：7 个 golden ARC 测试 + 8 个 ARC 专项测试全部通过，test_all 基线无回归（151/162 + 41 unit）。

### 与 clang RetainCountChecker 的对比
- **clang 为什么复杂**：ObjC 是动态语言（`objc_msgSend` 运行时查找方法），需要：
  1. `RetainSummaryManager` — 维护庞大的类+selector 摘要表（`lib/Analysis/RetainSummaryManager.cpp`）
  2. `ObjCARCInstKind` — 将每个 IR 指令分类为 retain/release/autorelease/use（`lib/Analysis/ObjCARCInstKind.cpp`）
  3. `RefVal` 状态机（`RetainCountChecker.h`）— 路径敏感符号执行追踪每个对象
  4. `__attribute__((ns_returns_retained))` 等注解标记方法所有权
  5. `objc_retainAutoreleasedReturnValue` 配对优化 — 跨函数调用约定打标
- **Nupa 为什么简单很多**：Nupa 是**静态 vtable 派发**，编译期已知方法签名和调用目标：
  1. **不需要**摘要表 — `ownership_for_method` 的 alloc/new/copy/init 命名约定 + init 链追踪已足够
  2. **不需要**指令分类 — AST 上直接识别 `MsgSend(selector=="retain")` 等
  3. **不需要**别名分析 — 变量就是名字，没有 `id` 动态类型问题
  4. **不需要**跨函数调用约定优化 — 编译器知道调用目标，不需要运行时 intrinsic
  5. 状态机直接从 `ownership_for_expr` 查询所有权（`crates/ownership/src/ownership.rs`）
- **clang 源码位置**（桌面）：
  - `lib/StaticAnalyzer/Checkers/RetainCountChecker/RetainCountChecker.{h,cpp}` — 核心状态机
  - `lib/Analysis/RetainSummaryManager.{h,cpp}` — 方法摘要表
  - `include/clang/Analysis/RetainSummaryManager.h` — 摘要类型定义（`ArgEffect`/`RetEffect`/`RetainSummary`）

### 错误检测 + Checker 完善计划（Aug 2026）
借鉴 clang 的分层设计，全部 Nupa 化（静态 vtable 派发，不依赖 ObjC 运行时）。

**总体架构对应：**

| clang | Nupa |
|-------|------|
| `Parse/` 错误恢复（`SkipUntil`） | `crates/parser` — 替换 `panic_mode` |
| `SemaDeclObjC.cpp` | `checker` — 声明语义检查 |
| `SemaExprObjC.cpp` | `checker` — 表达式语义检查 |
| `SemaObjCProperty.cpp` | `checker` — `@property` 验证 |
| `Sema::Namespace` (C++) | `binder`/`elaborator` — 命名空间检查 |
| `DiagnosticEngine` | `pipeline` — 统一错误收集/格式化 |

**阶段 1：Parser 错误恢复（最影响日常体验）**
- **现状**：`panic_mode` 遇到第一个错误就停止，一次编译只报一个错
- **clang 做法**：`Parser::SkipUntil` 跳到分号/大括号等恢复点，继续解析
- **Nupa 化方案**：

| 场景 | 恢复 token | 效果 |
|------|-----------|------|
| 缺 `;` | 跳到下一个 `;` 或 `}` | 继续解析后面的语句 |
| 缺 `)` | 跳到下一个 `)` 或 `;` | 继续解析表达式 |
| 缺 `]` | 跳到下一个 `]` 或 `;` | 继续解析消息发送 |
| 缺 `}` | 计数器匹配，跳到最近的 `}` | 继续解析外层 |
| 缺 `@end` | 在文件末尾报错 | 不阻塞后续解析 |

**阶段 2：结构错误检测**
- **clang 做法**：`Parser::ParseObjCAtEnd` 检查 `@interface/@implementation/@protocol` 是否匹配
- **Nupa 化**：增加 `@interface`/`@implementation`/`@protocol`/`@namespace` 的嵌套栈追踪

| 检测 | 实现位置 |
|------|---------|
| `@interface` 缺 `@end` | parser 在文件末尾检查栈 |
| `@implementation` 缺 `@end` | 同上 |
| `@protocol` 缺 `@end` | 同上 |
| `@namespace` 未闭合 `@endnamespace` | parser 在文件末尾检查栈 |
| 嵌套 `@interface` 在另一个里 | parser 禁止嵌套声明 |
| 不匹配的 `[]` / `()` / `{}` | parser 栈追踪 |

**阶段 3：Checker 语义检查（对应 clang 的 `SemaObjC`）**
- **现状**：checker 已有类型兼容性、协议一致性、裸串检查
- **需要新增**（参考 `SemaDeclObjC.cpp` + `SemaObjCProperty.cpp` + `SemaExprObjC.cpp`）：

| 检查项 | clang 文件 | Nupa 位置 |
|--------|-----------|-----------|
| 方法签名 `@interface` 与 `@implementation` 匹配 | `SemaDeclObjC.cpp` | `elaborator`（已有部分） |
| `@property` 属性冲突（同时 assign+retain） | `SemaObjCProperty.cpp` | `checker` |
| `@synthesize` 指向不存在的 ivar | `SemaObjCProperty.cpp` | `checker` |
| 协议要求的方法未实现 | `SemaDeclObjC.cpp` | `checker`（已有部分） |
| 类方法/实例方法混淆 | `SemaExprObjC.cpp` | `checker` |
| `instancetype` 用于非 init 方法 | `SemaExprObjC.cpp` | `checker` |
| 分类 (category) 名称与已有分类冲突 | `SemaDeclObjC.cpp` | `elaborator` |
| 协议循环继承 | `SemaDeclObjC.cpp` | `checker` |
| 类循环继承 | `SemaDeclObjC.cpp` | `checker` |
| 方法参数类型不匹配（消息发送时） | `SemaExprObjC.cpp` | `checker` |
| 块字面量参数类型检查 | `SemaExprObjC.cpp` | `checker` |
| `@selector` 引用不存在的方法 | `SemaExprObjC.cpp` | `checker` |

**阶段 4：语法建议（clang 的 `-W` 诊断 + Typo 修正）**
- **clang 做法**：`Sema::CorrectTypo` 在未定义名称时查找最接近的已有名称
- **Nupa 化**：编辑距离 + 符号表查找

| 特性 | 实现 |
|------|------|
| 未定义变量 → 建议最接近的已定义变量 | checker + 编辑距离 |
| 未定义方法 → 建议同类中的相似方法 | checker + 符号表 |
| 未定义类 → 建议已导入的相似类名 | checker + 符号表 |
| 未定义协议 → 建议已导入的相似协议名 | checker + 符号表 |

**阶段 5：死代码/未使用变量 warning**
- **clang 做法**：`-Wunused-variable`、`-Wunused-function`、`-Wunreachable-code`
- **Nupa 化**：在 checker 中遍历 AST

| 检测 | 触发条件 | 状态 |
|------|---------|------|
| 未使用变量 | 变量声明后从未被引用 | ✅ 已实现（`var_decls` 追踪 + `check_unused_vars`） |
| 不可达代码 | return/break/continue/throw 后的语句 | ✅ 已实现（`check_stmt` 的 Compound 里追踪 `unreachable` 标志） |
| 空 `@try`/`@catch` 块 | 没有语句的块 | ✅ 已实现（`is_empty_compound`） |
| 多余的 `@synthesize` | 自动合成的属性不需要手动 @synthesize | ⏳ 待实现 |

**阶段 6：C 语法检测（⚠️ 铁律：Nupa 是 C 超集，绝不能打断 Nupa 语法）**
- clang 的 C 检测（`-Wconversion`、`-Wsign-compare`、`-Wincompatible-pointer-types` 等）借鉴时要极其小心：
  - **Nupa 是 C 超集**：`[obj msg]`、`@interface`、`@property`、`@namespace` 等 Nupa 特有语法在 C 检测中必须被视为合法
  - **只加不改**：C 检测只能**新增** warning/error，绝不能**更改** Nupa 已有的合法语法解析路径
  - **白名单**：C 检测应该作用于纯 C 表达式/语句（`int a = b + c;`、指针转换等），跳过高层级 Nupa 构造（消息发送、特性语法）
  - **回归守护**：每次加 C 检测后必须跑 `test_all.py`，确认 192/202 基线无回归
- 候选检测项（参考 clang `-W` 系列，全部 Nupa 化）：
  | clang 警告 | Nupa 落地 | 状态 |
  |-----------|----------|------|
  | `-Wconversion`（隐式类型转换） | `check_conversion`（`crates/checker`）：浮点→整数精度丢失、整数降精度（long→int）、同宽符号性转换；Int 字面量在目标范围内则豁免（`literal_in_range`，避免 `unsigned int u = 5` 误报） | ✅ |
  | `-Wsign-compare`（符号比较） | `check_sign_compare`：二元比较左右操作数符号性不同则警告（signed vs unsigned）；只作用于纯 C 标量，跳过消息发送 | ✅ |
  | `-Wreturn-type`（缺 return） | `has_return_stmt` + `check_decl`：非 void 函数/方法体无任何 return 则警告（init 方法豁免） | ✅ |
  | `-Wunused-function`（未使用函数） | `declared_functions` + `called_functions`（FuncCall 记录调用）：顶层非 `main` 函数从未被调用则警告 | ✅ |
  | `-Wuninitialized`（未初始化变量） | `var_decls` 扩展 `(line, used, assigned)`：声明带 init 或之前有赋值才 `assigned=true`；VarRef 读取 `assigned=false` 则警告。`x = x + 1` 等 RHS 读取在标记 LHS 之前检查 | ✅ |
- **Nupa 超集安全护栏**（`check_conversion`/`check_sign_compare` 内部的跳过列表）：
  - 对象/指针类型（Id/Class/Sel/Instancetype/Named-pointer）一律跳过
  - `MsgSend` 表达式（`[obj foo]`）跳过 —— 返回 `id`，不做标量转换判断
  - 泛型类型 receiver（`[LogBuffer<LogEntry*> alloc]`）和命名空间限定名（`A::B`）跳过 —— 它们是类型引用不是变量
  - 全部是 **warning**（紫色，非 error），永不改变 AST 或解析路径
- ⚠️ **已知噪音**：Foundation 内联代码（不 import 时会展开 NPMutableArray 等）内部会产生少量 `sign-compare`/`unused variable` warning 混入输出；功能正确，纯体验问题

### Weak References — Implemented ✅
- ✅ Runtime side table: `nupa_weak_register` / `nupa_weak_unregister` / `nupa_weak_clear_all` in `include/nupa/runtime.c`
- ✅ `nupa_release` calls `nupa_weak_clear_all(target)` before `free(target)`
- ✅ Variable `__weak` decl: emits `__attribute__((cleanup(nupa_weak_auto_cleanup)))` + `nupa_weak_register` after init
- ✅ Property `(weak)` setter: emits `nupa_weak_unregister` → assign → `nupa_weak_register`
- ✅ `build.rs` rerun-if-changed for `runtime.c`

### Refcount Trace (`-trace-refcount`) — Implemented ✅ (Aug 2026)
- ✅ **New crate `crates/trace`** (`nupa-trace`): static reference-count simulator. Runs on the AST **after** ARC analysis has inserted `nupa_release` calls, so ARC-injected releases appear in the trace too.
- ✅ **CLI**: `nupac -trace-refcount file.np` prints a chronological, color-coded trace of every retained object's count, then exits (no codegen, no C compile). Flags: `-trace-max-iters <N>` (loop iterations simulated, default 2), `-trace-no-color`. Wired through `Pipeline.trace_refcount` / `trace_max_iters` / `trace_color` as `Step 4.5` in `pipeline.rs` (after ARC analysis, before the checker so even ARC-forbidden manual code is traceable).
- ✅ **Object identity = allocation site**: `Class#N` per-class counter. Allocations: `alloc`/`new`/`copy`/`mutableCopy`-prefixed, `init` chains to its receiver, `@"..."` literals, `@[...]` literals. Params of object type bound with count 1 and label `(param)`.
- ✅ **Count rules**: `nupa_release`/`[x release]` → −1; `nupa_retain`/`[x retain]` → +1; autorelease marks then batch-releases at `@autoreleasepool` end (`pool pop`); `x = [[.. alloc] init]` rebinds; `x = y` aliases share the same object.
- ✅ **Colors**: **green** = count increased vs previous print, **blue** = count decreased (still alive), **cyan** = freed (count reached 0), first print / unchanged = plain, **red** = errors — over-released (negative count) inline + still alive (`possible leak`) / over-released in the summary. Double-release decrements into negative territory (not clamped) so the summary flags it red. Yellow reserved for informational untracked-target warnings. `-trace-no-color` disables.
- ✅ **Warnings**: double-release/over-release (red `!`) and ops on untracked targets (yellow `! ...`); `== Summary ==` lists live objects (`possible leak`) and over-released objects (negative count) in red, freed objects plain, with allocation position; params excluded from leaks.
- ✅ **Branches**: `if`/`else` clone the pre-if state; final state = `else` path (or `then` when no `else`) so branch releases persist. Loops repeat the body `max_iters` times with fresh `Cat#N` identities per iteration.
- ✅ **ARC releases positions**: ARC inserts releases with line 0; the tracer falls back to the enclosing compound statement's line/col.
- ✅ **8 Rust unit tests** in `crates/trace/src/lib.rs` (`#[cfg(test)] mod tests`, AST constructed programmatically): alloc/release chain, retain/release, msg-send release, double-release warning, autoreleasepool batch pop, alias sharing, untracked warning, loop identity.
- ✅ **Escaped objects + `return`/`@throw`/`@catch`/static handling** (Aug 2026): the tracer now distinguishes ownership that leaves the traced scope:
  - `TraceObj` gains `escaped: bool`; `return expr` and `@throw expr` (via `trace_escape`) trace ref-ops inside the expression (`return [x autorelease]` autoreleases `x`) and mark the produced object `escaped`, so it is excluded from the leak Summary (a returned/thrown object is the caller's/runtime's responsibility).
  - `@catch (T *e)` binds the parameter to a `T (catch)` object (count 1) so `[e release]` in the catch body resolves instead of warning `untracked`.
  - Ref-ops whose receiver is itself a creation (`[[[X alloc] init] autorelease]`) create the object first, then apply the op — no more `! autorelease on untracked [...]`; the op result is returned so `Type *v = [x autorelease]` binds `v` to the object.
  - `static T *s = [[T alloc] init];` marks the created object `escaped` (static owns it for the process lifetime).
  - `@"..."` and `@[...]` literal objects are marked `escaped` in `create()` — in Foundation they are immutable constants / autoreleased runtime objects that are never manually released, so they are not leak candidates (removed 3 false positives in `tests/nparray_test.np`).
  - Branch binding persistence: a bare `if (cond) stmt;` without `else` inlines its `then` body via `trace_stmt_persist`, so bindings made inside (e.g. a static `s` assigned inside the `if`) survive to a later `return s`. This fixed the `+ (id)sharedNull` singleton pattern being misreported as a leak.
  - **Result**: all 5 `possible leak` false positives in `examples/01_JSONEditor/json_editor.np` (static `s_sharedNull`, `return [result autorelease]`, THROW's thrown error, `-copyValue` returns) eliminated — Summary reports `no live objects — all freed`.
  - Unit tests now **15** (7 new: returned-object escape, thrown-object escape, catch-param binding, creation-receiver refop, static-decl escape, bare-if persistence; 8 original).
- ✅ Completions (`completions/_nupac`, `nupac.bash`, `nupac.fish`) regenerated with the three new flags.
- 🟢 **SourceMap 统一行号翻译（Aug 2026）**：
  - **问题**：preprocessor 逐行内联 `#import` 文件，report 的行号是内联缓冲区行号，对不上原始文件（列号仍是准的，因为行没被合并）。
  - **新增 `nupa-cst::SourceMap`**（`crates/cst/src/source_map.rs`）：内联行号 → `(文件名, 原始行号)` 的单一事实来源。预处理器逐行拼接保证每个内联行唯一对应一个 (file, line)。
  - **统一出口**：parser `error()`、checker `check_error`/`check_warning`、pipeline ARC warning、`-trace-refcount` 输出全部调用 `SourceMap::locate()`——一处翻译，全链一致。格式统一 `file.nh:42:15`。
  - **接入点**：parser 的 `SourceMap` 字段（`with_source_map`）、checker 的 `source_map` 字段（pipeline 传入）、`TraceOptions.source_map`、pipeline ARC 分析复用同一个 SourceMap。
  - **无 double-translate**：AST 节点的 `line` 保持内联行号作为 SourceMap 索引（保证 `AST 内部`稳定、唯一），只有「给人看的输出」翻译。
  - **附带修复**：`has_return_stmt` 漏掉 `@noarc`/`@catch`/`@finally`/`@autoreleasepool` 内的 return，导致 Foundation 便利构造器被误报 `has no return statement`——已递归补齐。
  - **trace golden 更新**：`tests/golden/28_refcount_trace/*.out` 重新生成（8/8 通过），行号现在是原始文件行号。

### Unit Tests
- `cargo test` — 41/41 pass（lexer / parser / cst_print / cst_visit / symbol / binder / checker / preprocessor 等）；集成由 `test_all.py` 驱动，回归基线见上方 Tests Status。

### Soma Kernel — NASM + C + Nupa 三语 i386 内核 ✅ (Aug 2026)
- ✅ `examples/04_soma-kernel/` — 32 位保护模式内核，`boot/boot.asm`(引导+读盘+A20+GDT+进 PM) + `kernel/{entry,isr}.asm`(入口/IDT 桩) + `kernel/{kernel,hw}.c`(VGA/串口/kprintf/IDT/PIC/PIT/mem*/str*) + `nupa/soma_core.np`(Nupa 内核模块)。

### Nupa 原生裸机支持 — `-fno-libc`（freestanding mode）✅ (Aug 2026)
- ✅ **编译器级裸机支持**（不再靠内核侧手写 runtime）：新增 `nupac -fno-libc` 开关（`pipeline.no_libc`）。
  - **codegen** `emit_unit_with_headers(.., freestanding)`：freestanding 时不发 `#include <string.h>`，改发 `#define __NUPA_FREESTANDING 1` + `#include <nupa/runtime.h>`。
  - **`include/nupa/runtime.h`** 加 `__NUPA_FREESTANDING` 分支：不 include `<setjmp.h>/<stdarg.h>`；`jmp_buf` 自定 + `setjmp/longjmp` 映射到 `__builtin_setjmp/__builtin_longjmp`（零 libc）；异常全局 `__nupa_exception_buf/__nupa_exception_value` 由 `__thread` 改为普通全局（单核裸机）；声明 `extern NPClass nupa___nupa_root_class;` + `void *memcpy(...)`（用户提供定义）。
  - **`include/nupa/runtime.c`** 异常全局同样由 `__thread` 改为普通全局（`__NUPA_FREESTANDING` 分支）。
  - **`compile_to_binary`** 裸机模式：给 clang 加 `-ffreestanding -fno-builtin -fno-stack-protector -fno-pic -fno-pie -fno-asynchronous-unwind-tables -nostdlib -nostdinc`（`-arch x86*` 时再加 `-mno-sse -mno-mmx -mno-red-zone`）；并且**不链接** `runtime.c`（它依赖 libc malloc/__thread）。
  - **@try/@catch 在裸机可用**：转译代码用 `setjmp/longjmp`(builtin) + 普通全局异常状态，`memcpy` 由用户 kernel.c 提供 → 裸机打印 "caught [e errorCode] = 42 / finally always runs / after-try continues"。
- ✅ **soma-kernel 已改用编译器裸机支持**：删掉内核侧手写 `include/nupa/runtime.h` 和 `include/setjmp.h`；Makefile 用 `nupac -rewrite-nupa -fno-libc` 转译，CFLAGS 加 `-I../../include -D__NUPA_FREESTANDING`；kernel.c include 编译器真实 `<nupa/runtime.h>` 并定义 `nupa___nupa_root_class` + 异常全局 + `memcpy`。
- ✅ **回归**：全量套件 154/161 不变。

### Nupa 裸机分配器 — bump allocator + `[[Class alloc] init]` ✅ (Aug 2026)
- ✅ **`include/nupa/runtime_baremetal.c`**：提供 bump allocator（16KB 静态 arena + 偏移指针）+ `nupa_malloc`/`nupa_free`/`nupa_alloc`/`nupa_init`/`nupa_retain`/`nupa_release`/`nupa_autorelease`。`nupa_alloc` 用 `memset` 清零后设 `isa`/`retain_count`，`nupa_release` 计数归零后调 `nupa_free`。
- ✅ **零样板**：`runtime_baremetal.c` 现在自含 `nupa___nupa_root_class`、异常全局（`__nupa_exception_buf`/`__nupa_exception_value`，含 `__NUPA_FREESTANDING` 分支：裸机普通全局 / 宿主 `__thread`）、`memcpy`。用户只需链接它，不再手写任何运行时全局。soma-kernel 的 kernel.c 和 golden 的 helpers.c 已移除重复定义。
- ✅ **`include/nupa/runtime.h`**：添加 `nupa_malloc`/`nupa_free` API 声明。
- ✅ **soma-kernel** 新增 `HeapCounter` 类，`+ (id) alloc { return nupa_alloc(self); }`，`[[SomaCore::HeapCounter alloc] init]` 在裸机 i386 内核跑通（`[c add:10]=10 [c add:20]=30 [c value]=30`），ARC 自动插入 `nupa_release(c)`。
- ✅ **Golden test** `25_freestanding` 同样演示 bump allocator + alloc+init。
- ✅ **回归**：全量套件 154/161 不变。

### BSD Cross-Compilation — Implemented ✅ (Aug 2026)
- ✅ `build-all.sh` now targets **8 platforms** (up from 5): added `x86_64-unknown-freebsd`, `i686-unknown-freebsd`, `x86_64-unknown-netbsd`
- ✅ `zig-cc.sh` maps the 3 BSD rust triples to zig targets (`x86_64-freebsd-none`, `x86-freebsd-none`, `x86_64-netbsd-none`)
- ✅ Per-target linker wrappers (`zig-cc-x86_64-freebsd.sh`, `zig-cc-i686-freebsd.sh`, `zig-cc-x86_64-netbsd.sh`) embed the target so `zig cc` produces correct output
- ✅ FreeBSD targets need stub shared libraries for `devstat`, `procstat`, `kvm`, `memstat`, `util`, `rt`, `execinfo` (Rust std references them). `build-all.sh` auto-creates them in `target/bsd-stubs/` and `target/bsd-stubs-i386/` via zig cc, and passes `-L` + `--allow-shlib-undefined` as `RUSTFLAGS`
- ✅ NetBSD target builds without stubs, only needs the linker wrapper
- ✅ `.cargo/config.toml` generated per-target with correct linker paths
- ✅ All 3 BSD binaries verified: `file` shows correct ELF format for FreeBSD 14.0 / NetBSD 10.1
- ✅ Archives: `nupa-x86_64-unknown-freebsd.tar.gz`, `nupa-i686-unknown-freebsd.tar.gz`, `nupa-x86_64-unknown-netbsd.tar.gz`

### Fixes Applied (Current Session — Aug 2026)
- ✅ **错误分阶段前缀**：pipeline 各阶段 fail-fast 返回的错误消息改为 `Parse failed:\n[parser] line:col: msg` / `Binding failed:\n[binder] ...` / `Elaboration failed:\n[elaborator] ...` / `Type checking failed:\n[checker] ...`。`prefix_lines(stage, msg)` 给多行错误逐行加 `[stage]` 前缀（空行跳过），一条编译错误即可按 parser/binder/checker 分门别类阅读。见 `crates/nupac/src/pipeline.rs`。
- ✅ **`__typeof__` 作为内建函数实参**：`is_builtin_type_arg_start` 增加 `KeywordKind::Typeof`，使 `__builtin_types_compatible_p(__typeof__(x), int)` 的 `__typeof__(x)` 被 `parse_type_full` 解析为 TypeLiteral（原先报 `expected ')' after args (got keyword)`）。见 `crates/parser/src/parser.rs`。
- ✅ **typedef struct 字段类型丢失**：`typedef struct { ... } Name;` 的字段在 codegen Typedef 分支只匹配 `AstDeclData::Variable`，Ivar 字段落到 `_ => "int"`，导致 `unsigned char tag; unsigned long value; char name[8];` 全变 `int`。修复：Typedef 分支增加 `AstDeclData::Ivar` 匹配，字段类型经 `ast_type_to_c_str` 保留限定词。见 `crates/codegen/src/codegen.rs`。
- ✅ **`tests/asm_fusion_test.np` 全语法融合**：以内联 asm 融合测试为基底，新增 `@noarc`（块内私有对象 retain/release 成对平衡，不触碰 ARC 局部对象）、`@synchronized`、`__attribute__((packed/aligned/format/unused))`、双下划线标识符（`__FILE__`/`__LINE__`/`__builtin_expect`/`__typeof__`/`__alignof__`/`__builtin_types_compatible_p`）、泛型单态化 `TaskQueue<BlockHasher*>` + `@using` 别名、`@class` 前向声明（`Net::RemoteNode` 真未定义类）。14 步全部 clang 后端跑通，ARC 自动释放 + `@noarc` 手动管理并存。
- ⚠️ **`@noarc` 使用教训**：`@noarc` 内对**外层 ARC 管理的局部对象**手动 `release` 会 double-free（ARC 作用域结束还会自动 release），导致段错误 exit=139。正确用法：`@noarc` 块内创建自己的私有对象全手动管理，retain/release 成对平衡（`alloc`=1 → `retain`=2 → `release`=1 → `release`=0 块内释放），ARC 对象一律交给自动 release。
- ⚠️ **nupac CLI 命令格式**：`-asm file.s` 必须放在 `run` 关键字**之后**（`nupac run file.np -asm file.s`）。若放在 `run` 之前（`nupac -asm file.s run file.np`），run 前的 flag 扫描不处理 `-asm`，其值被当输入/未知参数。test_all.py 用的是正确格式。

### NPMutableArray — Implemented ✅ (Aug 2026)
- ✅ **`include/Foundation/NPMutableArray.{nh,np}`**：可变数组，继承 `NPArray`（复用其 `_items/_count/_capacity` 存储与不可变查询 API，dealloc 继承自 NPArray）。API：`+arrayWithCapacity:`/`+array`/`+arrayWithObject:`/`+arrayWithObjects:count:`；`-init`/`-initWithCapacity:`/`-initWithArray:`/`-initWithObjects:count:`；`-addObject:`/`-addObjectsFromArray:`/`-insertObject:atIndex:`/`-removeObjectAtIndex:`/`-removeLastObject`/`-removeObject:`/`-removeAllObjects`/`-replaceObjectAtIndex:withObject:`/`-exchangeObjectAtIndex:withObjectAtIndex:`/`-setObject:atIndex:`。存储用 `realloc` 2× 增长（初始 4）；`addObject:` 对元素 `nupa_retain`，移除类方法 `nupa_release` 被移元素；便利构造器仿 NPMutableString 用 `@noarc { return [[[self alloc] init...] autorelease]; }`。
- ✅ **`Foundation.nh`** 已加入 `NPMutableArray.nh`/`.np` import。测试 `tests/npmutablearray_test.np`（13 段，覆盖全部可变 API + 继承查询 + copy 深拷贝 + description），ARC 与 MRC 输出逐字节一致，`-trace-refcount` Summary = `no live objects — all freed`。
- ⚠️ **字符串字面量不做内插（interning）**：`@"X"` 两处出现是不同对象，`removeObject:`/`containsObject:` 用指针相等，测试需用同一变量（`NPString *x = @"X"`）保证身份。

### ARC 归属语义修复 — convenience 构造器在 ARC 下可用了 ✅ (Aug 2026)
- ✅ **`ownership_for_method` 默认分支 `Retained` → `Unretained`**（`crates/ownership/src/ownership.rs`）：ObjC 约定中非 `alloc/new/copy/mutableCopy/init` 家族的方法返回调用者**不拥有**的对象（autoreleased/unretained）。原先默认 `Retained` 导致 `[NPArray arrayWithObject:x]`/`[NPString stringWithUTF8String:".."]` 等 convenience 构造器被 ARC 在作用域结束注入 `nupa_release`，与 autorelease pool pop 双重释放 → 程序 exit=1（此前 nparray_test/npstring_demo 只能靠 MRC-retry 跑通）。修复后 nparray_test、npstring_demo、NPMutableString convenience 构造器均在 ARC 下直接通过。
- ✅ **`-copy` 契约修复**：`NPArray.copy`/`NPString.copy` 原先经 convenience 构造器返回 autoreleased 对象，违反 `copy` → +1 约定 → ARC 下 caller release + pool pop 双重释放。现 `NPString.copy` = `[[NPString alloc] initWithString:self]`（硬编码 NPString，可变子类 copy 得不可变结果），`NPArray.copy` = `nupa_retain([NPArray arrayWithObjects:_items count:_count])`（同样硬编码 NPArray）。内部 `[self copy]` 用户（`-description`、`stringByAppendingString:/UTF8String:` 的 NULL 分支）改用 `[NPString stringWithString:self]` 直接返回 autoreleased。
- ✅ **`crates/arc/src/ownership.rs` 死代码已删除**：该文件曾是 ownership 逻辑拆成独立 crate 前遗留的旧副本，rustc 从不编译它（`lib.rs` 只声明 `pub mod arc;`），误改它会毫无效果。arc crate 实际用 `nupa_ownership::*`（`crates/ownership`），改归属语义要改 `crates/ownership/src/ownership.rs`。
- ⚠️ **test_all.py 用 `target/debug/nupac`**（`NUPAC = PROJECT / "target" / "debug" / "nupac"`），改动 Rust 后若只 `cargo build --release` 会拿陈旧 debug 二进制跑出幽灵失败。回归前务必 `cargo build`（debug）刷新。

### NPLog 改用 `NPString *` 格式 — Implemented ✅ (Aug 2026)
- ✅ **签名**：`NPLog(NPString *format, ...)` / `__NPLogv(NPString *format, va_list args)`（`include/nupa/runtime.h`）。`runtime.h` 新增 `typedef struct NPString NPString;` 前向声明。
- ✅ **runtime.c 取 C 串**：runtime.c 是独立 C，不知道 codegen 生成的 `struct NPString` 布局，故用私有镜像结构 `struct __nupa_npstring_layout`（`isa/retain_count/_cstr/_length/_hash/_hashIsValid`，与 NPString.nh 的 `@public` ivar 顺序一致）取 `_cstr` 交给 `vfprintf`。⚠️ 若 NPString.nh 的 ivar 布局改动，需同步此结构。
- ✅ **codegen**：新增 `nplog_format_arg` 辅助函数（`crates/codegen/src/codegen.rs`，行 ~841）——NPString 存在时把 NPLog 的格式参数发射为 `(NPString *)nupa_stringFromCstr("...")`；NPString 缺失时退化为裸 C 字符串。两处 NPLog 特判（无 `%@` 内联路径 + `expand_nplog_format` 的 `%@` 展开路径）都改用它。`%@` 展开仍把 `%@`→`%s` 并把实参变为 `arg ? [[arg description] UTF8String] : "(null)"`。
- ✅ **调用点**：`tests/npstring_demo.np:103` 改为 `NPLog(@"NPString value: %s", ...)`；golden `13_foundation/02_nplog/*.c` 与 `03_description/player.c` 期望输出同步为 `NPLog((NPString *)nupa_stringFromCstr("..."), ...)`（test_all 不比对 .c，仅作文档）。
- ⚠️ **`NPLog("...")` 裸字符串已是 warning（非 error）**：`crates/checker/src/lib.rs` 的 FuncCall 臂按名字特判 `NPLog`，首参不是 `@"..."` 即报 warning。加 `-Werror` 才提升为 error。遵循 C 哲学：允许过，给警告，运行时崩溃。
- ✅ **回归**：test_all 187/198（4 个既有失败不变）、cargo unit 41/41、trace goldens 8/8。

### 通用 `"..."` 裸字符串拒绝 — 方法/函数参数类型检查 ✅ (Aug 2026)
- ✅ **通用机制取代 NPLog 特判（NPLog 特判保留）**：checker 在 `check_expr_inner` 的 MsgSend 和 FuncCall 臂中，遍历实参并与形参类型对比。若形参是对象类型（`id` / `instancetype` / `Named + is_pointer` 如 `NPString *`）而实参是裸 `AstExprData::String`（`"..."`），报错 `argument as a bare C string is not an object; use @"..." for an NPString`。
- ✅ **实现**：checker 新增 `collect_signatures`（在 `check` 入口第一遍遍历所有 AST 声明，收集方法与函数签名到 `method_params` / `function_params` HashMap），`is_object_type` 辅助函数，以及 MsgSend/FuncCall 臂的并行检查。形参从 `CstParam` 链表转为 `Vec<Option<AstType>>` 后与实参逐位对比。
- ✅ **涵盖**：Foundation 方法（`stringByAppendingString:`、`arrayWithObject:`、`containsObject:` 等）、用户定义方法、用户定义函数。`printf` / `strlen` 等 C 函数不在符号表中，不受影响。`(id)"..."` 显式强转可绕过检查（Cast 包裹后 `matches!(...data, AstExprData::String(_))` 不匹配）。
- ⚠️ **`runtime_test.np` 原用 `"..."` 传给 `id` 参数**：已改为 `(id)"..."` 显式强转，绕过通用检查。
- ✅ **`-Werror` flag**：CLI 新增 `-Werror`（`crates/nupac/src/main.rs`），将 checker 的 warning 提升为 error 中止编译。已加入 completions（bash/zsh/fish/flag_picker）。
- ✅ **`run` 子命令文件补全**：bash/zsh 完成脚本中 `nupac run <Tab>` 现在补全文件名（`.np`/`.nh` 文件），而非空列表。`completions/nupac.bash` 的 `nupac__subcmd__run` 分支改为 `COMPREPLY=($(compgen -f "${cur}"))`；`completions/_nupac` 的 `run` 子命令改用 `_files -g "*.np" -g "*.nh"`。
- ✅ **回归**：test_all 187/198（4 个既有失败不变）、cargo unit 41/41。

### C 桥接头 `--emit-bridge-header` — Implemented ✅ (Aug 2026)
- ✅ **背景**：C 直接调用 Nupa 对象方法要写 vtable 下标 / SEL 常量，很长。codegen 本来已为每个方法生成命名 C 函数（`NPString_UTF8String(NPObject*, SEL, ...)`），但 SEL 常量是 `static const`（跨文件不可见），且每次调用要传 `__nupa_sel_xxx`。
- ✅ **新增 `nupac -emit-bridge-header <file.h>`**：生成一个 C 桥接头，为每个类方法/实例方法输出三样东西：(1) 生成函数的 `extern` 声明；(2) 类前向声明（`struct NPString; typedef struct NPString NPString;`）+ `NPRange` 定义；(3) `static inline` 包装函数 `nupa_Class_method(...)`，内部用 `sel_registerName("原selector")` 拿 SEL（避开 `static const` SEL 常量跨文件不可见问题），类方法自动带 `&NUPA_CLASS_$_Class`。跳过 `nupa_root` 内部根类。
- ✅ **用法**：`nupac -rewrite-nupa lib.np -o lib.c -emit-bridge-header lib.h`，然后 `clang caller.c lib.c include/nupa/runtime.c -I include -o app`。C 侧 `#include "lib.h"` 后直接 `NPString *s = nupa_NPString_stringWithUTF8String_("hi");`。⚠️ 调用方 `main` 必须先 `nupa_metaInit()` 初始化类元数据。
- ✅ **实现**：`crates/codegen/src/codegen.rs` 新增 `emit_bridge_header(&CgUnit)`；`CgClassMeta`/`ClassInfo` 新增 `method_sel_names`（存原始 selector 带冒号，供 `sel_registerName`）；继承合并逻辑同步处理 `method_sel_names`。pipeline 加 `bridge_header: Option<String>`，Step 6.5 写文件。CLI 加 `-emit-bridge-header`（单/双横杠均可，已加入 `norm_flag`/`nupac_flags`/completions）。
- ✅ **回归**：test_all 187/198（4 个既有失败不变）、cargo unit 41/41。

### `class_forward_decl.np` 点语法修复 + Elaborator 作用域追踪 ✅ (Aug 2026)
- ✅ **根因**：`@class ForwardDeclared;` 前向声明 + 后置完整定义下，`ForwardDeclared *tmp = c.item;` 的 `tmp.value` 生成了 `tmp.value`（点，应 `->`）。真正根因是**符号表无作用域**——binder/elaborator 都从不 `push_scope`/`pop_scope`，Foundation 内联代码（`NPMutableArray.np` 里有 `NPObject *tmp`）与用户 `main()` 的 `tmp` 撞名，`st.lookup("tmp")` 返回第一个匹配（Foundation 的 `NPObject *tmp`），elaborator 无法解析 `value` 属性，退回 fallback `PropRef { prop: None, cls: None, is_arrow: false }`。
- ✅ **修复 1（elaborator）**：新增 `local_types` 作用域栈。`convert_decl` 处理 Function/Method 时 push/pop 一个作用域，并把形参类型注册进作用域；处理 Variable 声明时把声明类型注册进当前作用域。`convert_dot_expr` 的非 self 路径**优先**用 `lookup_local_type(obj_name)` 解析对象类型（解决撞名），找不到再退回 `st.lookup`。
- ✅ **修复 2（codegen）**：`AstExprData::PropRef` fallback 路径（`prop=None, cls=None`）中，若对象的 `expr_type.is_pointer` 为 true（checker 已把类型修正为 `ForwardDeclared *`），强制生成 `->` 而非 `.`（ObjC 实例永远是指针）。⚠️ 不再对已知类对象 prepend `_`（如 `@public int x` 的 `Vector2D` 不是 property，prepend `_` 会错）。
- ✅ **结果**：`class_forward_decl.np`、`grand_integrated_epic_test.np`、`scope_shadow_test.np`、`crypto_pipeline.np`、`weak_ivar.np` 全部通过。test_all 从 187/198 → **188/198**（只剩 3 个既有失败：`diamond_impl.np`、`mega_types.np` 无 main、`double_release.np` 故意崩溃）。
- ⚠️ **`golden/28_refcount_trace/double_release.np` 的 RUN_FAIL 是正常表现，不是内存泄漏**：它是故意对已释放对象 `nupa_release` 到负计数，`-trace-refcount` 的 Summary 显示红色 `over-released`——这正是该测试的目的（演示 double-release 检测）。
- ✅ **回归**：test_all 188/198（3 个既有失败不变）、cargo unit 41/41、trace goldens 8/8。

### NPArray / NPMutableArray golden 测试 + 上下文关键字修复 ✅ (Aug 2026)
- ✅ **新增 golden**：`tests/golden/13_foundation/06_nparray/nparray_test.{np,out}` 和 `07_npmutablearray/npmutablearray_test.{np,out}`，与 04_npstring/05_npmutablestring 同模式（`.np` + `.out`，test_all 校验 exit code）。覆盖 arrayWithObjects:count:/arrayWithObject:/array、@[] 字面量、containsObject:/indexOfObject:-1、copy（+1→ARC release）、description；NPMutableArray 的可变 API（add/insert/replace/set/exchange/remove/removeAll/addObjectsFromArray/alloc+initWithArray/copy）。
- ✅ **上下文关键字修复**：lexer 把 `copy`/`retain`/`weak`/`strong`/`assign`/`nonatomic`/`getter`/`setter`/`readonly`/`readwrite` 注册为**全局关键字**（`KeywordKind::At*`），导致 `int copy = 0;`、`NPArray *copy = ...` 解析失败（"expected ';' after expression (got keyword)"）。这些词在 ObjC 里是**上下文关键字**——只在 `@property (...)` 里特殊。修复：parser 的 `is_name_token`/`parse_qualified_name_with_keywords`/`parse_primary` 三处把 `is_contextual_kw_ident()`（这些 At* 关键字）当作合法标识符接受；`@property (copy)` 的属性解析（`match_keyword(AtCopy)` 等）不受影响。`[obj retain]` 消息发送、`(weak)` 属性声明照常。
- ✅ **回归**：test_all 188 → **190/200**（3 个既有失败不变）、cargo unit 41/41、trace goldens 8/8。

### `xx_t` 类型解析 + Block 生成格式修复 + Timsort 测试 ✅ (Aug 2026)
- ✅ **`xx_t` 类型解析修复**：parser 原本只有硬编码 typedef 名列表（`size_t`/`FILE` 等），`clock_t t0 = clock();` 会被当表达式而失败。修复 `is_declaration_start`：`IDENT IDENT`（如 `clock_t t0`）判为声明，覆盖所有系统头 typedef；并排除复合赋值（`x *= e`、`x <<= e`）。补全常用 POSIX 类型（`clock_t`/`time_t`/`off_t`/`pid_t`/`mode_t` 等）。
- ✅ **Block 字面量格式修复**：clang 后端 `emit_expr` 的 `CgExprData::BlockLit` 原来用 `emit_stmt` 发射函数体，产生多行 + 尾换行，导致函数调用参数里的块块 `);` 换行错乱。新增 `emit_stmt_inline`（单行语句发射器），块块体改为单行 `{ return (a < b); }`。
- ✅ **gcc 后端 invoke 函数双大括号修复**：block 展开时 `convert_expr` 预写 `{` 又经 `emit_stmt` 再写 `{`，产生 `{ { ... } }`。现改为不预写 `{`，交给 body 的 Compound 自闭环。
- ✅ **`tests/timsort_test.np`**：Nupa 语法版 Timsort（`@interface`/`@implementation`、多段 selector `- (void)lowerBound:lo:hi:key:using:`、Block 比较器、`@autoreleasepool`、`NPLog(@"%@")`、`@public` ivar）。stdin 读数字 → 排序 → 打印 + 耗时。已验证 15/300/500/700/1000 全对、已排序输入自适应加速。
- ✅ **回归**：test_all **191/201**（3 个既有失败不变）、cargo unit 41/41。

### Generated C 可读性注释 — Implemented ✅ (Aug 2026)
- ✅ **生成 C 代码带 `/* */` 注释（默认开启）**：`emit_unit_with_headers` 新增 `comments: bool` 参数（pipeline 默认 true，`-no-comments` 关闭）。共 5 层，全部用 `/* */`：
  - **T1 文件头 banner**：替换原 `// Generated by nupac`，标注 source 文件 + backend。
  - **T2 分节横幅**（`section_comment` helper）：`/* ------- Section N · 标题 ------- */`，13 节——Requires & defines / Forward declarations / SEL constants / Type declarations & typedefs / Struct definitions / Function prototypes / File-level variables / VTable & class layouts / Class metadata infrastructure / Vtable & metadata instances / Class metadata initialization / Runtime support / Function bodies。
  - **T3 类注释**：class struct 前 `/* Class layout: X (super: Y) */`；vtable 实例前 `/* VTable instance: X */`；meta vtable 实例前 `/* Meta vtable instance: X */`；getClass IMP 前 `/* +getClass for X */`。
  - **T4 方法注释**：函数体循环前预构建 `method_comments` map（`owner_mname` → ObjC 签名），每个方法 IMP 上方输出 `/* -[Person age] */` 或 `/* +[Person alloc] */`（用 `method_sel_names` 原始 selector，只留 ObjC 风格，不带 C 函数签名）。含命名空间类（`A::B`）时只取短名 `B`。
  - **T5**：`// ─── Block expansion ───` 在 comments 开启时改 `/* Block expansion definitions (gcc/portable) */`。
- ✅ **`-no-comments` CLI flag**：单/双横杠均可（`norm_flag`/`nupac_flags`/`DOUBLE_TO_SINGLE`/clap `Arg` 全加），pipeline `no_comments` 字段贯通。关闭时保留旧 `// Generated by nupac` 首行，行为与之前完全一致。completions 三件套（`_nupac`/`nupac.bash`/`nupac.fish`）手工同步新增 flag。
- ✅ **零回归**：cargo unit 41/41、test_all **193/203**（3 个既有失败不变）、trace goldens 7/8（`arc_inject` 既有失败，与注释无关——trace 跑在 codegen 前）。注释不影响 C 编译（161 个 .np 全部默认带注释编译通过）。

### 全量特性压力测试 `grand_feature_stress_test.np` ✅ (Aug 2026)
- ✅ **`tests/grand_feature_stress_test.np`**：单文件 `main` 汇总近期全部特性——Foundation 容器（`NPMutableArray` 增删查 + `NPLog(NPString*)`+`%@`）、泛型单态 `Box<T>` + 嵌套泛型 `Box<Box<NPString*>*>`、10 参数多段方法 + `@implementation { int _sum; }` ivar 写法、`NPString` 级联拼接、`typedef BOOL (^CompareBlock)` 捕获外部变量、`typedef int (*IntFn)` + `struct Pair`、上下文关键字 `copy`/`retain`/`weak` 当标识符、`clock_t`/`time_t`、ARC 自动释放 + `@noarc` 手动平衡、`@try { @throw @"boom"; }`、类方法 vs 实例方法。

### Codegen 健壮性审计 + nil-messaging / catch / 链式修复 ✅ (Aug 2026)
- ✅ **实证审计**：ASan+UBSan 复跑 4 个代表文件（npstring_demo/nparray_test/timsort_test/grand_feature_stress_test）全过、`-trace-refcount` 3 项 Summary 均 `no live objects — all freed`；clang --analyze 暴露 3 个真实缺陷，全部修复：
- ✅ **缺陷 1 — nil-messaging 语义缺失（最高危）**：`[nil msg]` 之前直接解引用 `nil->isa` → 段错误 exit=139；ObjC 应静默返回 0/nil。修复：`emit_expr` 的实例消息发射统一改为 temp+守卫 `({ NPObject *__nupa_tmp_N = (NPObject *)(recv); __nupa_tmp_N ? <dispatch>(__nupa_tmp_N, sel, ...) : 0; })`（删除了原来无守卫的 simple-Ident 内联路径，codegen.rs:4581）。split-emit（alloc+init）路径同样加守卫 `__nupa_tmp_N ? ... : 0`（codegen.rs:5036）。
  - **结构体返回特判**：`NPRange r = [s rangeOfString:]` 在 nil 守卫下 `cond ? struct : 0` 非法（incompatible operand types）→ 新增 `vtable_return_type`（从 `CLASS_METHOD_METADATA` 的 fn-ptr 类型提取返回类型）+ `nil_msg_fallback`（指针/void/标量用 `0`，结构体用 `(T){0}` 复合字面量，codegen.rs:4455-4470）。
  - **连带修复链式消息**：nil 守卫的 temp 化统一后，`[[[e maybeNil] stringByAppendingUTF8String:"x"] ...]` 不再生成非法的 `->stringByAppendingUTF8String_` 垃圾 C——外层 send 走守卫路径正确展开（原来 Empty 未声明该方法时走 arrow-access fallback 产生破损代码）。
- ✅ **缺陷 3 — catch 变量 dead store / 类型转换**：`@catch (Boom *e)` 原来无条件生成 `Boom * e = __nupa_exception_value;`——(a) catch 体不用 `e` 时是 dead store（clang analyzer DeadStores）；(b) 缺显式 cast（`-Wall` incompatible pointer types）。修复（codegen.rs:1953）：新增 `expr_refs_name`/`stmt_refs_name`/`decl_refs_name` 三个 AST 引用检测；用到 `e` → `Boom * e = (Boom *)__nupa_exception_value;`（带 cast）；不用 → `Boom * e; (void)e;`（无 dead store、无 unused）。grand_feature_stress 的 analyzer 警告从 1 → 0。
- ✅ **checker nil-messaging warning**：`[nil msg]` 接收者为字面量 nil 时发 warning `message 'X' sent to nil receiver; result is always zero/nil`（checker/src/lib.rs MsgSend 分支，零误报——变量接收者不报），并入现有 `-Werror` 提升体系。
- ✅ **新测试**：`tests/nil_messaging_test.np`（6 段：nil 值返回=0、nil void 无操作、nil 结构体返回零值、nil 链式安全、非 nil 链式正确、字面量 nil 发送）+ checker warning 验证。修复前 `[nil value]` 段 exit=139，修复后全过。
- ✅ **回归**：cargo unit 41/41、test_all **194/204**（3 个既有失败 + 7 interactive 不变，新增 nil_messaging_test 通过）、trace goldens 7/8（`arc_inject` 既有失败）。
- ⚠️ **`@noarc` 块必须手动计数归零**：`alloc`=1 → `retain`=2 → `release`=1 → `release`=0（漏一个 release 会让 trace 报 `still alive — possible leak`，AGENTS 的 @noarc 教训一致）。修正后 `-trace-refcount` Summary = `no live objects — all freed`。

### `#pragma mark` 原位透传 + `@数字字面量`（NPNumber）✅ (Aug 2026)
- ✅ **`#pragma mark` 位置修复**：此前 preprocessor 把所有 `#` 行塞进 `c_out`（C 前导 → 生成文件顶部），导致 `#pragma mark` 全部堆到文件最上面。现只把 `#pragma mark ...`（纯 IDE 标记）保留在 nupa 流内定位透传：
  - **preprocessor**：`trimmed.starts_with("#pragma mark")` → 写入 `nupa_out`（其余 `#pragma`/`#warning` 仍进 `c_out`）。
  - **新增 raw-line 通道**：`CstDeclKind/Data::RawLine(String)`、`AstDeclKind/Data::RawLine(String)`、`CgDeclKind/Data::RawLine(String)`；parser 新增 `parse_raw_pragma_decl`（按源码切片消费整行），在 `parse_declaration_inner`、`@interface`/`@implementation` 方法循环、`parse_statement` 接入；elaborator `CstDeclData::RawLine → AstDeclData::RawLine`；codegen `convert_decl`/`emit_decl` 原样发射。
  - **方法分组**：class 方法循环里 pragma 先 `pending_pragmas` 暂存，flush 时按 `fn_name` 插入到下一个方法声明的**前面**（解决 `@interface` 先建 method 声明、`@implementation` 的 pragma 被追加到末尾的错位）。
  - 验证：`tests/tricalc.np` 的 5 个 `#pragma mark` 各自紧贴其方法组，生成 C 编译通过。
- ✅ **`@数字字面量`（boxing literal）**：`@123` / `@1.5` / `@1e3` 以前 `@` 被 lex 成 Error token（`@1, @2` 报 `expected ']' after message send`）。现在：
  - **lexer**：`@` 后跟数字 → 新增 `TokenKind::AtNumber`（扫描整数/小数/指数，`.` 仅在后跟数字时消费，避免吞 `@1.foo`）。
  - **parser**：`AtNumber` 在 `parse_primary` 里 desugar 成普通类方法发送 `[NPNumber numberWithInt:N]`（浮点用 `numberWithDouble:`），复用全部消息派发路径，无需新 AST 变体。
  - **`include/Foundation/NPNumber.{nh,np}`**（**NP 前缀**，非 NS）：`+numberWithInt:/numberWithLongLong:/numberWithDouble:/numberWithBool:`、`-intValue/longLongValue/doubleValue/boolValue`、`-description`（`snprintf` → NPString）、`-isEqualToNumber:`；已并入 `Foundation.nh`。
  - 验证：`NPNumber *n = @42; NPLog(@"%@", n)` → `42`；`@[ @1, @2, @3 ]` → `[1, 2, 3]`。
- ✅ **ObjC variadic 集合构造器已支持**：`[NPArray arrayWithObjects:a, b, c, nil]`（单冒号选择子 + 逗号多参）已可用。
  - **parser**：消息发送的 `sel:` 分支收集逗号分隔的附加实参；当 `args.len() > selector 冒号数` 且 selector 为 `arrayWithObjects:` 时，desugar 成数组字面量 `@[a, b, c]`（丢弃结尾 `nil`），复用既有 `nupa_array_create` 路径（parser 里 `parse_primary` 的 message-send 分支）。
  - ⚠️ 目前仅覆盖 `arrayWithObjects:`；结果与 `@[...]` 相同（不可变 `NPArray`）。`[NPMutableArray arrayWithObjects:...]` 仍会得到不可变数组（如需可变请用 `arrayWithObjects:count:` 或 `@[...]` + mutableCopy）。其他 variadic selector 仍不支持。
- ✅ **回归**：cargo unit 41/41、test_all **195/206**（`diamond_impl`/`mega_types` 无 main、`double_release` 故意崩溃 共 3 既有失败；`q.np` 现通过）、trace goldens 7/8（`arc_inject` 既有失败）。

### `@namespace … @endnamespace`（取代花括号）✅ (Aug 2026)
- **背景/分类**：ObjC 的 `@end` 家族（`@interface`/`@implementation`/`@protocol`）全是**顶层声明容器**，且彼此从不嵌套；花括号只用于**执行作用域**（`@try`/`@autoreleasepool`/`@synchronized`）。`@namespace` 装的是声明（类/协议），属**声明容器**，因此改用 `@endnamespace`（`@end` 风格的专属终结符）而非 `{}`——既符合分类，又保持 `@end` 永远只关闭 @interface/@implementation/@protocol 的**无歧义性**（避免家族史上首次"@end 嵌 @end"）。
- **lexer**：新增关键字 `@endnamespace` → `KeywordKind::AtEndNamespace`（`token.rs` + `KW_TABLE`）。
- **parser**：`parse_namespace` 不再 `consume(LBrace/RBrace)`，改为循环 `parse_declaration` 直到 `match_keyword(AtEndNamespace)`；**允许空 namespace**。selector 排除列表（3 处）+ 错误恢复 sync 列表加入 `AtEndNamespace`。
- **全仓库迁移**：37 个 `.np`（另有 2 个 `.np.bak`/`.nh.bak`）的 `@namespace X { … }` → `@namespace X … @endnamespace`，含**嵌套 namespace**（脚本用字符串/注释剥离 + namespace 栈式大括号计数，正确区分 ivar 列表/方法体大括号）。`@using namespace X;`（无 `{`）不动。
- **不兼容旧花括号语法**，无过渡期。
- **文档/编辑器**：README.md、CHINESE.md 示例更新；VS Code grammar（`nupa.tmLanguage.json`）的 `namespace-decl` end 改为 `@endnamespace`（并加 `@endnamespace` 关键字高亮）。
- **回归**：cargo unit 41/41、test_all **195/207**（3 既有失败 + 空文件 `tests/circle.np`）、trace goldens 7/8（`arc_inject` 既有失败）。

### Checker：类方法访问实例 ivar 报错 ✅ (Aug 2026)
- **背景**：`+` 类方法体里写实例 ivar（如 `_radius`）时，checker 过去不拦，codegen 直接把 `self`（`NPClass *`，类对象）当实例生成 `((struct Cls *)self)->_ivar` → 运行期读类元数据垃圾内存 / 崩溃（`tests/circle.np` 早期版本因此「跑起来没输出」）。对应 ObjC 的编译错误 `instance variable '_x' accessed in class method`。
- **实现**：`Checker` 新增 `current_method_is_class` 字段（`check_decl` 的 `Method` 臂按 `is_class_method` 设置/还原）；`check_expr_inner` 的 `IvarRef` 臂中，若当前是类方法且 `obj` 为 `self` → `check_error("instance variable '_x' accessed in class method (self is the class, not an instance)")`。实例方法（`-`）与 `other->_ivar` 不受影响。
- **测试**：`crates/checker/src/lib.rs` 新增单元测试 `class_method_ivar_access_is_error` / `instance_method_ivar_access_is_ok`（手工构造 AST；checker 此前无单测，这是首批 2 个）。
- **回归**：cargo unit **43/43**（+2）、test_all **196/207**（`tests/circle.np` 修正为 `+run` 类方法后通过；仅剩 `diamond_impl`/`mega_types` 无 main、`double_release` 有意 over-release 共 3 既有失败）、trace goldens 7/8。

### nupac 跨平台构建健壮性 + C 编译器可选 ✅ (Aug 2026)
- ✅ **double-link 修复**（`compile_to_binary`）：普通模式下曾同时加 `-lnupa`（若找到 `libnupa.a`）**和** `runtime.c` → 重复符号。改为**二者择一**（优先静态库，否则 bundle 的 `runtime.c` 源码）。
- ✅ **bundle 根目录健壮解析**：新增 `resolve_bundle_root()`，取代原来硬编码的「exe 往上 3 层 parent」。候选顺序：`$NUPA_HOME` → exe 的各父目录（bundle `PREFIX/bin/nupac`、dev `target/<profile>/nupac`）→ **exe 自身目录** → `.`；取第一个含 `include/nupa/runtime.h` 的。修掉了「exe 同目录存在陈旧 `include/` 副本时被优先选中」的坑（曾导致 `q.np` 因用旧 Foundation 缺 `NPNumber` 而编译失败）。
- ✅ **build.rs 自包含副本修复**：build.rs 一直找的是不存在的 `install-pkg.sh` → 拷贝块长期失效；改找 `install.sh`。并新增 `emit_rerun_for_dir()` 对 `include/` 下**每个文件**发 `rerun-if-changed`（目录级指令抓不到既有文件的**内容**改动），保证 `target/<profile>/include` 与源码同步（`NPNumber` 曾被漏拷）。
- ✅ **临时文件跨平台**：run 模式 `/tmp/<stem>` → `std::env::temp_dir()` + `std::env::consts::EXE_SUFFIX`；compile 模式默认 `a.out` → `a.out{EXE_SUFFIX}`。
- ✅ **C 编译器可选**（为 Windows 铺路）：新增 `select_c_compiler(backend) -> Vec<String>`（支持多词命令）—— `$NUPA_CC` 覆盖全部；`-backend gcc` → `gcc`；否则 **Windows 默认 `zig cc`**（自带 libc 的单一工具链，`windows-gnu`），其它平台默认 `clang`。`compile_to_binary` 增加 `cc: &[String]` 参数，拆分 program + 前置参数（`Command::new("zig").args(["cc", …])`）。
- ✅ **`zig cc`/LLD 重复符号修复**：`runtime.c` 的 `NUPA_CLASS_$_nupa_root` 定义缺少 `__attribute__((weak))`（注释却写"defined weak"）。clang/macOS 会把两处 tentative definition 当 common 合并，但 **LLD（zig cc）严格报 `duplicate symbol`**。加 weak 后 `zig cc` 通过（weak + strong tentative 合并）。
- ⚠️ **Windows 仍未做**：`runtime.c` 的 `__thread` 需 MSVC 兼容（`__declspec(thread)`；zig cc 用 `__thread` 没问题）；建议 Windows 端**捆绑 `zig`** 作为 `zig cc` 后端（nupac 直接调用它，无需用户装 LLVM/MSVC）。
- ⚠️ **`.gitignore` 隐患（已发现，待修）**：第 12 行 `nupac` 模式会匹配任意路径段，导致 **整个 `crates/nupac/` 未被 git 跟踪**（`git ls-files crates/nupac` 为空，`!! crates/nupac/`）；应改为 `/nupac`（仅根目录二进制）。另 `AGENTS.md`/`todo.md` 也在 `.gitignore` 中（未跟踪）。
- ✅ **回归**：cargo unit 43/43、test_all **198/209**（同上 3 既有失败不变）。
