#!/usr/bin/env bash

mkdir -p .logs
mkdir -p research

todo_list_file="${1:-TODO.md}"

loop_rules="$(cat <<EOF
# Rules
- Only do a single task from the todo checklist.
- Never ask for confirmation from the user, this is non interactive. Just do a single item, no more no less
- If you do research for a future item, write your research to the ./research directory, then edit the todo checklist with a reference to that research file
- When you are done with the item, validate that you completed the task. Fix problems until you are satisfied with the code quality.
- After you have validated the quality of the code, check the item off the todo checklist
- After you check an item off the checklist, append what you have done to CHANGELOG.md
- After you have updated CHANGELOG.md, write a commit message and commit your code to git
- If you are working on a task and it is very large or you are having a hard time completing it, break the task down into more todo items and append them to the todo checklist
- If all todo items are done in the entire todo checklist file, then create an empty \`.done\` file
- Do not create temporary files in /tmp/, use .tmp/ instead
EOF
)"

echo -e "${loop_rules}\nTodo checklist: @${todo_list_file}" |
  opencode run --attach http://localhost:4096 |
    tee -a .logs/$(date +%s).log

git push

if [ ! -f .done ]
then
  exec ./loop.sh
fi

rm .done

