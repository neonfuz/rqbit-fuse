#!/usr/bin/env bash

mkdir -p .logs
mkdir -p research

todo_list="$1"
if [ -z "$1" ]
then
	todo_list=TODO.md
fi

while [ ! -f .done ]
do
	echo @LOOP.md "@${todo_list}" | opencode run --attach http://localhost:4096 | tee -a .logs/$(date +%s).log
done

rm .done
