[-> 中文](CHINESE.md)

<div align="center">
<img src="doc/assets/Nupa_avatar.svg" alt="Nupa_avatar" width="210">

# The Nupa Programming Language

[**View Project Examples**](#project-examples)

[Overview](#overview) · [Why Nupa?](#why-nupa) · [Project Examples](#project-examples) · [Quick Start](#quick-start) · [Language Features](#language-features) · [New Features](#new-features) · [Compilation & CLI](#compilation--cli) · [Code Examples](#code-examples) · [Design Principles](#design-principles) · [Roadmap](#roadmap) · [FAQ](#faq)

</div>

---

> ⚠️Warning
> **The GitHub Releases page lags behind the source code.** The releases shown
> on the GitHub website are usually *older* than what's pushed to the `main`
> branch — I often forget to cut a new release after pushing new commits. If you
> want the latest features and fixes, **clone the repo and build from source**
> (see [Build](#build)); only use the downloaded releases if you prefer the
> stable, older snapshot.

---

## **Overview**

Nupa is a **purely static** Objective-C dialect (C superset language). Nupa source is transpiled to C99, then compiled to native machine code by Clang. No runtime message forwarding, no GC pauses, no JIT warm-up — all method dispatch, memory management, and polymorphism are resolved at compile time. It currently works — there are games and tools running in it. If you find it interesting, feel free to give it a try.

I don't intend to replace ObjC or Swift. I just miss ObjC's syntax and wanted to let it live again in a statically compiled world. ☺️

---

## Why Nupa?

I simply like ObjC's message send syntax `[obj message]`. ObjC's runtime (`objc_msgSend`) is heavy, and I wanted to write ObjC-like code that compiles straight to C — so Nupa was born: ObjC syntax compiled statically, no runtime dependency, generating clean C.

This is not a production-ready language. It's a toy, exploring the question: "what happens if you transpile ObjC into plain static C?"

### What it does

- Method calls → compile-time VTable offsets, no objc_msgSend
- Memory management → CFG static analysis, retain/release decided at compile time
- Output → human-readable C99, not compiler IR

### Design Goals

- **Fun**: that's the most important one
- **Readable**: generated C is meant to be read by humans
- **Lightweight**: just one small static runtime

---

## Project Examples

[![examples](https://img.shields.io/badge/examples-000?style=for-the-badge)](examples/)

`examples/`

| Project               | Description                                                                              | Run                            |
| --------------------- | ---------------------------------------------------------------------------------------- | ------------------------------ |
| **`04_soma-kernel/`** | Tiny 32‑bit i386 OS kernel (NASM + C + Nupa), bare‑metal `-fno-libc` mode                | `./run.sh` or `./run.sh --gui` |
| **`03_LibUI/`**       | GUI app via [libui-ng](https://github.com/libui-ng/libui-ng), all callbacks in pure Nupa | `./run_libui.sh`               |
| **`02_ncurses/`**     | Terminal demos (`ncurses_demo`, `sysmon`) using `Terminal::Ncurses`                      | `make run`                     |
| **`01_JSONEditor/`**  | Multi‑file JSON editor with split‑screen terminal preview                                | `nupac run json_editor.np`     |

---

## Quick Start

### Dependencies

- Clang (>= 14)
- Rust (stable, with cargo)
- Git

### Build

```bash
git clone https://github.com/3shine123/nupa-lang.git
cd nupa-lang
cargo build --release
```

### Install

The build automatically drops an `install.sh` (plus headers and `libnupa.a`) next to the `nupac` binary. Install it to your system with:

```bash
# After building from source — the script lives next to the binary
cd target/release        # or target/debug if you ran a plain `cargo build`
./install.sh             # installs to /opt/nupa by default
./install.sh /usr/local  # optional: pick a different prefix
```

This installs:

- **binary** → `<prefix>/bin/nupac`
- **static lib** → `<prefix>/lib/libnupa.a`
- **headers** → `<prefix>/include/`
- **system headers** → `/usr/local/include/{Foundation,nupa}/` (needs write permission; skip with `sudo` or pass a second arg like `./install.sh /opt/nupa ~/include`)

The installer auto-detects your language (中文 / English).

Alternatively, download a prebuilt release archive (`nupa-<platform>.tar.gz` or `.zip`) from the releases page, extract it, and run the `install.sh` inside:

```bash
tar xzf nupa-x86_64-unknown-linux-musl.tar.gz
cd nupa-x86_64-unknown-linux-musl
./install.sh
```

> **Tip:** with `nupac` on your PATH and system headers installed, `<nupa/runtime.h>` and `<Foundation/...>` resolve automatically — no `-I include` needed.

### Compile a Nupa Program

```bash
# Just output C code (auto-derives .np → .c)
nupac -rewrite-nupa hello.np
nupac hello.np -rewrite-nupa               # flag works anywhere
nupac -rewrite-nupa hello.np -o out.c      # explicit path also works
# (--rewrite-nupa double-dash form also accepted)

# Compile the transpiled C alone with Clang — two ways:
#   1) compile the runtime source directly
clang -I include -o hello hello.c include/nupa/runtime.c
#   2) link the prebuilt libnupa.a (lives next to the nupac binary)
clang -I include -o hello hello.c -Ltarget/release -lnupa

# Output object file directly
nupac hello.np -o hello.o                  # -c mode, no linking

# Compile to executable
nupac hello.np -o hello_bin                # transpile + compile + link

# Compile + run
nupac run hello.np
nupac run hello.np -o hello_bin            # keep binary after run
nupac run hello.np                          # auto-clean temp binary

# Show compilation warnings
nupac -v run hello.np

# [!] Error: .c output without -rewrite-nupa
nupac hello.np -o hello.c   → Error: use -rewrite-nupa to output C code

# [!] Error: no output method specified
nupac hello.np              → Error: specify -o or -rewrite-nupa
```

### Shell Completion (Tab autocomplete)

`nupac` ships with generated completion scripts for **zsh**, **bash** and **fish**, built with
[clap_complete](https://crates.io/crates/clap_complete). Regenerate them any time with:

```bash
nupac -gen-completions zsh > _nupac
nupac -gen-completions bash > nupac.bash
nupac -gen-completions fish > nupac.fish
```

The scripts are also copied into the install bundle (`share/nupac/completions/`) by `install.sh`.

**zsh** — add the directory to `fpath` before `compinit` runs:

```zsh
fpath=(/opt/nupa/share/nupac/completions $fpath)
autoload -U compinit && compinit
```

**bash**:

```bash
source /opt/nupa/share/nupac/completions/nupac.bash
```

**fish**:

```fish
source /opt/nupa/share/nupac/completions/nupac.fish
```

After installing a new version, clear the zsh cache with `rm -f ~/.zcompdump*` and open a new terminal.

### Run Tests

```bash
# Run all tests
./test_all.sh -j4

# Run Rust unit tests
cargo test --workspace
```

---

## Language Features

### Class System

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

### Protocol

```nupa
@protocol Drawable
- (void)draw;
- (BOOL)isVisible;
@end

@interface Shape : NPObject <Drawable>
@end
```

### Properties

```nupa
@interface Person : NPObject
@property NPString *name;
@property int age;
@property (readonly) NPString *identifier;
@end
```

### Category

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
    // temp is released when the pool pops
}
```

### @selector

```nupa
SEL sel = @selector(doSomething:);
```

### Full C Compatibility

```nupa
#include <stdio.h>
#include <stdlib.h>

@interface Wrapper : NPObject
- (void)callCFunction;
@end
```

### C Attributes (__attribute__)

Nupa supports `__attribute__((...))` pass-through. You can write C `__attribute__` on global declarations and struct fields, and the compiler preserves them verbatim in the generated C output.

```nupa
__attribute__((packed))
struct Point {
    int x;
    int y;
};

__attribute__((format(printf, 1, 2)))
int my_log(const char *fmt, ...);
```

The compiler ships with a **590-attribute classification table** (scraped from Clang and GCC official docs). The `-backend` option controls which attributes are allowed:

| Option                        | Behavior                                                                            |
| ----------------------------- | ----------------------------------------------------------------------------------- |
| `-backend=clang` (default)    | Allow clang-specific attributes (e.g. `availability`, `diagnose_if`, `objc_direct`); gcc-only attributes error |
| `-backend=portable`           | Only attributes supported by both gcc and clang; others error                        |
| `-backend=gcc`                | Allow gcc-specific attributes (e.g. `strub`, `optimize`, `stack_protect`)            |

Unknown attributes (not in the table) produce a warning and pass through — never a hard error.

### Memory Management

Nupa uses **compile-time static ARC**. The compiler determines each object reference's lifetime through CFG dataflow analysis and inserts retain/release calls automatically. No manual `retain`/`release`/`autorelease` needed.

In MRC mode (`-fno-nupa-arc`):

```nupa
NPObject *obj = [[NPObject alloc] init];
// ... use obj ...
[obj release]; // MRC manual release
```

### C Bridge (`-emit-bridge-header`)

Nupa transpiles to C, but calling Nupa object methods from C normally requires verbose vtable-index and SEL-constant boilerplate. `-emit-bridge-header` generates a header with `static inline` wrappers for every method, so C code can call Nupa objects like ordinary C functions.

**Usage**: transpile a Nupa library to C, then generate the bridge header:

```bash
nupac -rewrite-nupa lib.np -o lib.c -emit-bridge-header lib.h
```

Then include the bridge header from C:

```c
#include "lib.h"

int main(void) {
    nupa_metaInit();  // required before using any Nupa objects

    // Class method: nupa_<Class>_<method>(params...)
    NPString *s = nupa_NPString_stringWithUTF8String_("Hello");

    // Instance method: nupa_<Class>_<method>(self, params...)
    size_t len = nupa_NPString_length(s);
    const char *cstr = nupa_NPString_UTF8String(s);

    // Nested message send (like Nupa's [[s UTF8String] ...])
    const char *nested = nupa_NPString_UTF8String(
        nupa_NPString_stringWithUTF8String_("nested")
    );

    // Multi-argument message send (like Nupa's [arr replaceObjectAtIndex:0 withObject:obj])
    NPArray *arr = nupa_NPArray_arrayWithObject_(s);
    nupa_NPArray_replaceObjectAtIndex_withObject_(arr, 0, s);

    // Each colon in the selector becomes an underscore in the function name:
    //   [obj foo:arg1 bar:arg2] → nupa_<Class>_foo_bar_(obj, arg1, arg2)
    //   [m replaceCharactersInRange:rng withString:str]
    //   → nupa_NPMutableString_replaceCharactersInRange_withString_(m, rng, str)
}
```

Link against the transpiled `.c` and `runtime.c`:

```bash
clang caller.c lib.c include/nupa/runtime.c -I include -o app
```

⚠️ The caller's `main` must call `nupa_metaInit()` first. The bridge header uses `sel_registerName` to resolve selectors at runtime, so it does **not** depend on the codegen-generated `static const` SEL constants (which are file-local and invisible across translation units).

#### Memory Management from C

Nupa's **ARC is compile-time and applies only to `.np` source** — it never sees calls coming from C. When C code calls bridge functions, objects are **not** automatically retained or released. Manage them manually, following the ObjC memory-management naming convention:

| Method family                                  | Caller owns?       | What C code must do                                                             |
| ---------------------------------------------- | ------------------ | ------------------------------------------------------------------------------- |
| `alloc`, `new`, `copy`, `mutableCopy`          | ✅ +1               | Must call `nupa_release(obj)` when done                                         |
| `init`                                         | ❌ consumes `alloc` | Nothing                                                                         |
| everything else (e.g. `stringWithUTF8String:`) | ❌ autoreleased     | Nothing, but `nupa_retain(obj)` if it must outlive the current autorelease pool |

```c
#include "lib.h"

int main(void) {
    nupa_metaInit();
    nupa_autoreleasepool_t *pool = nupa_autoreleasepoolPush();

    // +1 (returns autoreleased convenience object); use within this pool only
    NPString *s = nupa_NPString_stringWithUTF8String_("hello");
    printf("%s\n", nupa_NPString_UTF8String(s));

    // If it must outlive the pool: retain now, release later
    NPString *t = nupa_NPString_stringWithUTF8String_("world");
    nupa_retain(t);
    nupa_autoreleasepoolPop(pool);      // t survives (was retained)
    printf("%s\n", nupa_NPString_UTF8String(t));
    nupa_release(t);

    // alloc-family returns +1 → must release
    NPString *u = nupa_NPString_alloc(nupa_NPString_stringWithUTF8String_("x") /* placeholder */);
    // (real usage: nupa_NPString_copy(s) returns +1, release it)
    NPString *copy = nupa_NPString_copy(s);
    nupa_release(copy);
}
```

`nupa_retain`, `nupa_release`, `nupa_autorelease`, `nupa_autoreleasepoolPush`/`nupa_autoreleasepoolPop` are declared in `<nupa/runtime.h>` and work on any Nupa object. This is exactly the manual-retain-count (MRC) model — from the C side you can think of Nupa objects as raw pointers you own or don't own by convention.

---

## New Features

Nupa adds features on top of Objective-C syntax that ObjC itself doesn't have.

**Recent highlights:**

- **Native bare-metal support (`-fno-libc`)** — compiles to self-contained C with no libc, no Foundation, no TLS; `@try/@catch` uses `__builtin_setjmp/longjmp`, and a zero-boilerplate `runtime_baremetal.c` provides the bump allocator, `NUPA_CLASS_$_nupa_root`, exception state, and `memcpy`.
- **C superset** — `@protocol` + conformance, `@property` + `@synthesize`, `instancetype`, `@public` ivars, dot syntax, structs + function pointers, inline asm, C-style casts.
- **Typed `@catch`** — each catch block now checks `isa == &NUPA_CLASS_$_Class`, so only the matching class enters the handler; multiple catches are properly isolated.
- **ARC fixes** — scope-stack model no longer releases parent-scope variables at nested scope end; `for`-init object hoisting stops leaks and invalid `for` headers.
- **`@noarc` block** — block-level MRC: in ARC mode, manual `retain`/`release`/`dealloc`/`autorelease` inside `@noarc { }` is allowed; the block-level analogue of `-fno-nupa-arc` and clang's `-fno-objc-arc`.
- **`__attribute__` pass-through + `-backend`** — full support for C `__attribute__((...))` and all `__`-prefixed C predefined identifiers (`__FILE__`, `__LINE__`, `__builtin_*`, `__extension__`, `__typeof__`, `__alignof__`, ...); the `-backend` flag controls which compiler-specific attributes are allowed.

### Implicit Root Class (`nupa_root`)

Nupa now supports user-defined root classes. You no longer need to inherit from `NPObject` — an `@interface` without a superclass automatically gets a compiler-injected implicit root class `nupa_root`, while keeping `id` type uniformity and static dispatch.

**Before:**

```nupa
@interface Animal : NPObject   // had to inherit NPObject
```

**After:**

```nupa
@interface Animal              // no superclass → implicit root class
@interface Animal : NPObject   // explicit NPObject still works
```

Both are valid, and `id` can point to any Nupa object.

#### How It Works

When no superclass is specified, the compiler injects `nupa_root`:

```nupa
// User code:
@interface Animal {
    int age;
}
- (void)speak;
@end

// Compiler treats as:
@interface Animal : nupa_root {
    int age;
}
- (void)speak;
@end
```

Generated C code:

```c
// Built-in structures
struct nupa_object_header {
    struct nupa_vtable *vtable;
};

struct nupa_root {
    struct nupa_object_header header;
};

// Animal's struct
struct Animal {
    struct nupa_root __super;  // contains header
    int age;
};
```

#### `id` Type

```c
typedef struct nupa_root *nupa_id_t;
```

`id` is no longer tied to `NPObject` — it only requires the object to start with `nupa_root`. This means:

```nupa
Animal *a = [[Animal alloc] init];
id obj = a;                    // valid: Animal inherits from nupa_root
[obj speak];                   // static dispatch: obj->header.vtable[...]
```

#### Explicit Inheritance Still Works

```nupa
@interface Dog : Animal {
    NSString *breed;
}
@end
```

Generated C:

```c
struct Dog {
    struct Animal __super;     // contains nupa_root → header
    struct NSString *breed;
};
```

#### `NPObject` vs `nupa_root`

| Declaration                 | Means                             | Use Case                        |
| --------------------------- | --------------------------------- | ------------------------------- |
| `@interface Xxx`            | Implicit `nupa_root`, lightweight | Custom layout, kernel, embedded |
| `@interface Xxx : NPObject` | Explicit NPObject, full runtime   | User apps, ARC, retain/release  |

```nupa
// Lightweight root class, no refcounting overhead
@interface KernelTask {
    int pid;
    int priority;
}
- (void)run;
@end

// Full NPObject with automatic memory management
@interface UserModel : NPObject
@property NSString *name;
@end
```

#### Bare-Metal / Freestanding Support (`-fno-libc`)

Nupa can compile to **self-contained C with no libc, no Foundation, no TLS**, for kernels, MCUs, and bare-metal embedded development.

```bash
nupac -rewrite-nupa -fno-libc kernel.np   # emits self-contained C
```

In `-fno-libc` mode the transpiled C:

- does **not** `#include <string.h>`; instead `#include <nupa/runtime.h>` (freestanding branch)
- implements `@try/@catch/@finally` with `__builtin_setjmp/longjmp` (zero libc), with plain (non-`__thread`) exception globals
- is self-contained for `SEL`/`NPClass`/`NPObject`/`id`

The user only provides: `NUPA_CLASS_$_nupa_root`, the exception globals (if using `@try`), `memcpy` (if using `@try`), and freestanding headers (`stdint.h`/`stddef.h`/`stdbool.h`).

**Bare-metal allocator + `[[Class alloc] init]`** (`include/nupa/runtime_baremetal.c`):

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
    HeapCounter *c = [[HeapCounter alloc] init];  // bare-metal heap alloc
    [c add:10];                                    // → 10
}
```

Features verified bare-metal (`examples/04_soma-kernel/` i386 protected-mode kernel + `tests/golden/25_freestanding/`):

- `@namespace` + `@interface` (implicit root class)
- Class / instance method messaging
- `@try/@catch/@finally`
- `@selector`, inline asm, C-style casts
- `[[Class alloc] init]` heap allocation + ARC auto-`nupa_release`

Sample output (soma-kernel under qemu):

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

#### Method Dispatch

All Nupa objects dispatch through a unified VTable mechanism:

```c
// [obj doSomething:arg]
obj->header.vtable[INDEX_doSomething](obj, arg);
```

The compiler assigns a fixed global index to each selector. All classes place the function pointer for the same selector at the same VTable position. If a class doesn't implement a method, the slot holds the parent's implementation or NULL.

#### Kernel-Friendly Design

The object header is minimal:

```c
struct nupa_object_header {
    struct nupa_vtable *vtable;
    // no retain count, no flags
};
```

Reference counting is managed by compile-time static ARC analysis, not stored in the object. `nupa_id_t` is a plain C pointer (8 bytes on 64-bit), zero ABI overhead for passing, assigning, and array storage.

#### Status

   Implemented:

- [x] Implicit root class injection (semantic analysis)
- [x] `nupa_root` and `nupa_object_header` C code generation
- [x] `id` → `nupa_id_t` type mapping
- [x] Unified VTable index allocation
- [x] Root/subclass struct generation
- [x] Unit test coverage

### @namespace

`@namespace` organizes classes, functions, and constants, avoiding global name collisions. This is a feature ObjC lacks — traditional ObjC relies on prefix conventions (e.g., `NS`, `UI`) to simulate namespacing.

```nupa
@namespace Game
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
@endnamespace

@namespace UI
    @interface HUD : NPObject {}
    - (void)showPlayerHealth:(Game::Player *)player;
    @end
@endnamespace
```

**Encoding rules**: `::` separators are encoded as `__` in C symbols.

| Nupa Symbol                   | Transpiled C Symbol          |
| ----------------------------- | ---------------------------- |
| `Game::Player`                | `Game__Player`               |
| `Game::Entities::Enemy`       | `Game__Entities__Enemy`      |
| Method `-[Game::Player init]` | `Game__Player_init`          |
| VTable                        | `NUPA_VTABLE_$_Game__Player` |
| Class metadata                | `NUPA_CLASS_$_Game__Player`  |

**Features**:

- Nested namespaces supported (`Game::Entities::Enemy`)
- Cross-namespace references (`Game::Player *player`)
- `@class` forward declarations inside namespaces (`@class Player;`) — codegen emits `struct Game__Player;` + `typedef struct Game__Player Game__Player;` so forward-declared types are usable in method signatures
- Cross-namespace inheritance (`@interface HUD : Engine::Graphics::Renderable`)
- No prefix convention needed — C symbols are encoded automatically
- Classes without namespaces remain backward-compatible

#### @using Import Mechanism

`@using` imports symbols from other namespaces into the current scope, avoiding the need to write fully qualified names each time. Three forms are supported:

**Form 1: Import a fully qualified name**

```nupa
@using Game::Player;
Game::Player *p = [[Game::Player alloc] init];
// After @using, the short name Player can be used instead
Player *p = [[Player alloc] init];
```

**Form 2: Import with an alias**

```nupa
@using GP = Game::Player;
// GP is an alias for Game::Player
GP *p = [[GP alloc] init];
```

**Form 3: Import an entire namespace**

```nupa
@using namespace Game;
// All classes under Game can be accessed by short name
Player *p = [[Player alloc] init];
Enemy *e = [[Enemy alloc] init];
```

**Conflict detection**:

- If a short name conflicts with an existing symbol in the current scope, the compiler reports an error
- If two `@using` entries import the same short name, the compiler reports an ambiguity error
- Aliases and short names are valid within the file scope of the `@using` declaration

### `@noarc` Block — Block-level MRC

In ARC mode, the checker forbids manual memory management:

```nupa
[obj release]; // error: explicit 'release' not allowed in ARC mode
```

`@noarc { }` scopes a block where you manage memory manually — the block-level analogue of `-fno-nupa-arc` (and clang's `-fno-objc-arc`):

```nupa
@noarc {
    [obj retain];
    [obj release];
    [obj autorelease];
}
```

Key points:

- **Block-level scope** — only statements inside `@noarc { }` are exempt. Everything outside still uses static ARC, and manual `retain`/`release`/`dealloc`/`autorelease` outside the block is a compile error.
- **No ARC injection** — the ARC analyzer skips `@noarc` blocks entirely, inserting no retain/release for objects used there.
- **Runtime-method exemption** — the implementations of `retain`/`release`/`dealloc`/`autorelease` themselves may call these methods without `@noarc`.
- **Whole-program analogue** — `-fno-nupa-arc` switches the whole program to MRC; `@noarc` does the same for a single block.
- **Foundation** — the NPString/NPMutableString convenience constructors (`+stringWithUTF8String:`, `+stringWithString:`) wrap their deliberate `autorelease` in `@noarc { }`.

---

### Refcount Trace (`-trace-refcount`)

`-trace-refcount` runs a static reference-count simulator over the AST **after** ARC injection, printing a chronological, color-coded trace of every retained object's count, then exits without codegen or compilation. It is a debug aid for verifying that each object is released exactly once (no leaks, no double-releases).

```bash
nupac -trace-refcount app.np                          # color trace
nupac -trace-refcount -trace-no-color -trace-max-iters 2 app.np
```

Options:

- **`-trace-no-color`** — disable ANSI colors (for diffs / CI piping).
- **`-trace-max-iters <N>`** — how many loop iterations each loop simulates (default 2); every iteration gets fresh `Cat#N` object identities.
- Colors: **green** = count increased · **blue** = count decreased (still alive) · **cyan** = freed (reached 0) · **red** = double-release/over-release and `possible leak`/`over-released` in the summary · **yellow** = informational untracked-target warnings.
- Object identity = allocation site (`Class#N` per class); creations are `alloc`/`new`/`copy`/`mutableCopy`-prefixed, `init` chains to its receiver, and `@"..."`/`@[...]` literals.
- Objects that leave the traced scope are excluded from the leak summary: `return`/`@throw` results, `static` singletons, `@"..."`/`@[...]` literals, and method parameters. A final `== Summary ==` reports live (`possible leak`), over-released (negative count), and freed objects with their allocation positions.

---

## Compilation & CLI

### Command-Line Options

```bash
nupac [options] <input.np>

Modes:
  (none)            Default: transpile + compile to binary (requires -o)
  run               Transpile + compile + run (auto-clean temp binary)

Options:
  -o <file>         Output file (.o produces object file, otherwise executable)
  -I <dir>          Add include search path
  -L <dir>          Add library search path
  -v, --verbose     Show verbose output (including Clang warnings)
  -V, --version     Show version number
  -rewrite-nupa     Output C code only (no compilation)
  -fnupa-arc        Enable ARC (default)
  -fno-nupa-arc     Disable ARC (manual MRC mode)
  -fno-checker      Skip type checking
  -fno-libc         Bare-metal/freestanding output (no libc, no TLS)
  -backend <mode>   C compiler backend: clang (default), portable, or gcc
  -arch <target>    Build for target architecture (e.g. -arch x86_64)
  -asm <file.s>     Link a real assembly file (repeatable)
  -gen-completions <shell>  Generate shell completion script (zsh|bash|fish)
  -emit-bridge-header <file.h>  Generate a C bridge header for calling Nupa from C

Refcount trace (debug aid):
  -trace-refcount                Print a static reference-count trace of each retained object, in source order
  -trace-max-iters <N>           Loop iterations simulated in the trace (default 2)
  -trace-no-color                Disable colors in the trace
```

### Build System Integration

**cargo**:

```bash
cargo build
cargo test --workspace
```

---

## Code Examples

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

### Polymorphism

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
        [animals[i] speak];  // VTable static dispatch
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

### Static Generics

Nupa compiles generics at compile time via **monomorphization** — each `DataPack<QuantumToken *>` becomes a standalone C struct `DataPack_QuantumToken_ptr` with concrete type substitutions. No type erasure, no boxing, no runtime overhead.

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
        // Each instantiation generates specialized C code
        DataPack<QuantumToken *> *tokenPack = [[DataPack<QuantumToken *> alloc] init];
        DataPack<EncryptedMetric *> *metricPack = [[DataPack<EncryptedMetric *> alloc] init];
    }
    return 0;
}
```

**How it works**:

- `DataPack<T>` → `struct DataPack` (generic) is skipped; only specialized structs are emitted
- `DataPack<QuantumToken *>` → `struct DataPack_QuantumToken_ptr` with `QuantumToken * _storage[2]`
- Methods are cloned per instantiation with substituted return/param/body types
- VTable, meta VTable, and class metadata are generated per instantiation
- Type name encoding: `DataPack<QuantumToken *>` → `DataPack_QuantumToken_ptr`

**Status**: ✅ Fully implemented for single type parameter. Multiple parameters (`<K, V>`) in progress.

---

## Design Principles

### 1. Static > Dynamic

ObjC's runtime is powerful, but I don't want to depend on it. Make all decisions at compile time — what you generate is what runs, no surprises.

- Method dispatch → VTable offsets
- Memory management → CFG static analysis
- Protocol conformance → compile-time checks

### 2. Generate Human-Readable C

Nupa's "backend" is **human-readable C99**, not LLVM IR. This means:

- Debug with standard Clang/LLDB tools
- Generated C can be reviewed, modified, embedded in other projects
- No LLVM backend lock-in — wherever Clang runs, Nupa runs

### 3. Incremental

Start from a class system, add things gradually:

- ✅ Class/Protocol/Category/Properties
- ✅ Block / @autoreleasepool
- ✅ Static ARC
- ✅ @selector / VTable polymorphism
- ✅ @namespace
- ✅ Exception handling (`@try`/`@catch`/`@finally`/`@throw` via `setjmp`/`longjmp`)
  - ⚠️ Known limit: an object that lives across a **cross-function throw** leaks (the `longjmp` skips its scope-end `nupa_release`); same-function `@throw` is handled. ARC-aware unwind is planned.
- ⏳ Foundation standard library
- ⏳ Compiler self-hosting

### 4. Readability

Generated C should be as clear as handwritten C:

- `struct` + `->` for ivar access
- `static const SEL` constants
- Consistent and predictable naming
- Explicit temporary variable names

---

## Roadmap

### Phase 1: Infrastructure ✅

- [x] Lexer
- [x] Preprocessor
- [x] Parser
- [x] CST validation & printing

### Phase 2: Semantic Analysis ✅

- [x] Symbol table
- [x] Name binding
- [x] Type checking
- [x] Property elaboration
- [x] Protocol conformance

### Phase 3: VTable + Object Layout ✅

- [x] VTable layout
- [x] Object memory layout
- [x] Class metadata

### Phase 4: Intermediate Representation ✅

- [x] Typed AST
- [x] CST → AST
- [x] CFG construction

### Phase 5: Static ARC ✅

- [x] Ownership inference
- [x] Local + global ARC
- [x] Retain/Release insertion
- [x] ARC verification

### Phase 6: C99 Code Generation ✅

- [x] C99 AST
- [x] AST → C99 conversion
- [x] Header generation
- [x] Compiler options

### Phase 7: Runtime ✅

- [x] Core retain/release/alloc/init
- [x] Autorelease pool

### Phase 8-10: In Progress

- [ ] Foundation standard library

- [x] Block runtime

- [x] Weak references

- [✅] Generics (monomorphization)

- [ ] Exception handling

- [ ] Debug information

- [x] VSCode/IDE support

---

## FAQ

### **Is it production-ready?**

Not yet. But it is **real** - it compiles, it runs, and it is designed with growth in mind. If you find syntax appealing and want to contributem, you are welcome.

### What can Nupa do?

Write small games, tools, toys. The snake game, Flappy Bird, space shooter, tic-tac-toe in this repo are all written in Nupa, running in the terminal.

### What's missing compared to ObjC?

- No `objc_msgSend` — VTable static dispatch
- No runtime Method Swizzling
- No `forwardInvocation:`
- Selectors are compile-time constants, not runtime strings

### Why C99 as output?

Because C99 compiles everywhere. Generate human-readable C, compile with Clang, debug with lldb. No need to bind to any specific backend.

---

## License

MIT License

Copyright (c) 2026 3shine123