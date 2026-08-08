#!/bin/bash
exec zig cc -target x86_64-freebsd-none "$@"
