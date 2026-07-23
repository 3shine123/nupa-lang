project('nupa', 'c',
  default_options : [
    'c_std=c99',
    'warning_level=3',
    'werror=false',
  ])

fs = import('fs')
nupa_version = fs.read('VERSION').strip()

# Pass version to all compilation units
add_project_arguments('-DNUPA_VERSION="' + nupa_version + '"', language : 'c')

# --- Transpiler (nupac) ---
nupac_src = files(
  'transpiler/src/main.c',
  'transpiler/src/lexer.c',
  'transpiler/src/preprocessor.c',
  'transpiler/src/cst.c',
  'transpiler/src/cst_visit.c',
  'transpiler/src/parser.c',
  'transpiler/src/symbol.c',
  'transpiler/src/binder.c',
  'transpiler/src/checker.c',
  'transpiler/src/layout.c',
  'transpiler/src/elaborator.c',
  'transpiler/src/ast.c',
  'transpiler/src/cfg.c',
  'transpiler/src/ownership.c',
  'transpiler/src/arc.c',
  'transpiler/src/codegen.c',
)

nupac = executable('nupac', nupac_src,
  include_directories : [include_directories('include'), include_directories('transpiler/include')],
  install : true)

# --- Runtime (libnupa) ---
libnupa_src = files(
  'runtime/src/nupa_retain.c',
  'runtime/src/nupa_release.c',
  'runtime/src/nupa_alloc.c',
  'runtime/src/nupa_class.c',
  'runtime/src/nupa_autoreleasepool.c',
  'runtime/src/nupa_log.c',
  'runtime/src/nupa_weak.c',
)

libnupa = static_library('nupa', libnupa_src,
  include_directories : include_directories('include'),
  install : true)

# --- Tests ---
test_src = files(
  'tests/unit/test_lexer.c',
  'tests/unit/test_preprocessor.c',
  'tests/unit/test_parser.c',
  'tests/unit/test_cst_print.c',
  'tests/unit/test_cst_visit.c',
  'tests/unit/test_symbol.c',
)

# Additional test executables not in test_src loop but built manually
# (test_binder, test_category, test_checker, etc. are compiled directly by ninja)
additional_tests = files(
  'tests/unit/test_codegen.c',
  'tests/unit/test_codegen_convert.c',
  'tests/unit/test_codegen_emit.c',
  'tests/unit/test_codegen_header.c',
  'tests/unit/test_binder.c',
  'tests/unit/test_category.c',
  'tests/unit/test_checker.c',
  'tests/unit/test_elaborator.c',
  'tests/unit/test_layout.c',
  'tests/unit/test_cfg.c',
  'tests/unit/test_ownership.c',
  'tests/unit/test_arc.c',
)

test_lib = static_library('test_lib', nupac_src,
  include_directories : [include_directories('include'), include_directories('transpiler/include')])

foreach t : test_src
  test_name = t.full_path().split('/')[-1].split('.')[0]
  test_exe = executable('test_' + test_name, t,
    include_directories : [include_directories('include'), include_directories('transpiler/include')],
    link_with : test_lib)
  test(test_name, test_exe)
endforeach

# Build additional test executables (for test_all.sh, not meson test)
foreach t : additional_tests
  test_name = t.full_path().split('/')[-1].split('.')[0]
  executable(test_name, t,
    include_directories : [include_directories('include'), include_directories('transpiler/include')],
    link_with : test_lib)
endforeach