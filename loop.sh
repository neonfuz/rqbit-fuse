#!/usr/bin/env bash

mkdir -p .logs
mkdir -p research

todo_list="${1:-TODO.md}"

while [ ! -f .done ]
do
  echo -e "Instructions: @LOOP.md\nTodo checklist: @${todo_list}" | \
    opencode run --attach http://localhost:4096 | \
      tee -a .logs/$(date +%s).log
  git push
done

rm .done
