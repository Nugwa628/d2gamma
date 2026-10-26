#!/bin/sh
# The default build wrapper of tools/ci/bincache.py: runs its arguments as they are.
# Point D2_BUILD_QUEUE at your own wrapper to limit parallel builds on a shared machine.
exec "$@"
