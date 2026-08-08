#!/bin/bash
exec zig cc -target x86-freebsd-none "$@"
