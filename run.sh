#!/bin/bash
# set environment variable PK_VENV=/path/to/venv or put 'venv' in this directory
# set debug=1 if you're having trouble getting it working. It logs to /tmp/pk.log

debug=0

venv=$PK_VENV
[ -z "$venv" ] && venv=venv

DIR="$(pwd)"
if [[ "$(readlink -f "$0")" != "$0" ]]; then
    cd "$(dirname "$(readlink -f "$0")")"
else
    cd "$(dirname "$0")"
fi

TITLE="File Picker"
MODE="file"
MIME_LIST=""

[ $debug = 1 ] && echo "launcher args: $*" > /tmp/pk.log
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
      echo "Invalid option: -$opt -$OPTARG" >> /tmp/pk.log
      exit 1
      ;;
  esac
done

if [[ ! "$MODE" =~ ^(file|files|dir|save)$ ]]; then
  echo "Error: Invalid mode flag value (-m). It should be one of [file files dir save]." >> /tmp/pk.log
  exit 1
fi

if [ ! -f "$venv/bin/activate" ]; then
cat << EOF > /dev/stderr
You may need to set up a venv. Put a venv in this directory or set environment variable PK_VENV to /path/to/venv.
Try this:
python -m venv $venv
pip3 install -r requirements.txt
EOF
else
	. $venv/bin/activate
fi

#not using title flag but you can if you want
cmd="python ./pikeru.py \
	--mode '${MODE}' \
	--path '${DIR}' \
	--title '${TITLE}' \
	--mime_list '${MIME_LIST[@]:-}' \
	--parent '${PARENT:-}'" 
[ $debug = 1 ] || echo "cmd: $cmd" >> /tmp/pk.log
eval $cmd
