#!/bin/bash

# Special argument handling.
if [ "$#" -eq 1 ]; then
  if [ "$1" -eq "-" ]; then
    cd -
    exit 0
  fi
  if [ "$1" -eq "--help" ]; then
    warpto --help | less
    exit 0
  fi
fi

OUTPUT=$(warpto "$@")
if [ $? -eq 0 ]; then
  cd "$OUTPUT"
  exit 0
else
  echo "$OUTPUT"
  exit 1
fi
