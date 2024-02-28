#!/bin/bash

venv=~/stuff/v1/bin/activate
debug=1

TITLE="File Picker"
MODE="file"
DIR="$(pwd)"
MIME_LIST=""

[ -z $debug ] || echo "launcher args: $*" > /tmp/pkerr
while getopts "e:t:m:p:i:" opt; do
  case $opt in
    e)
      PARENT="$OPTARG"
      ;;
    t)
      TITLE="$OPTARG"
      ;;
    m)
      MODE="$OPTARG"
      ;;
    p)
      DIR="$OPTARG"
      ;;
    i)
      MIME_LIST="$OPTARG"
      ;;
    \?)
      [ -z $debug ] || echo "Invalid option: -$opt -$OPTARG" >> /tmp/pkerr
      exit 1
      ;;
  esac
done

if [[ ! "$MODE" =~ ^(file|files|dir|save)$ ]]; then
  [ -z $debug ] || echo "Error: Invalid mode flag value (-m). It should be one of [file files dir save]." >> /tmp/pkerr
  exit 1
fi

[ -z "$venv" ] || . ~/stuff/v1/bin/activate

#not using title flag but you can if you want
cmd="python ./pikeru.py \
	--mode '${MODE}' \
	--path '${DIR}' \
	--mime_list '${MIME_LIST[@]:-}' \
	--parent '${PARENT:-}'" 
[ -z $debug ] || echo "cmd: $cmd" >> /tmp/pkerr
cd `dirname $0`
eval $cmd
