#!/bin/bash
# set environment variable PK_VENV=/path/to/venv or put 'venv' in this directory
# set debug=1 if you're having trouble getting it working. It logs to /tmp/pk.log

debug=0
logfile=/tmp/pk.log

# LD_PRELOAD is set by the browser and can interfere with an opencv lib used to capture video frames
unset LD_PRELOAD

venv=$PK_VENV
[ -z "$venv" ] && venv=venv

DIR="$(pwd)"
cd "$(dirname "$(readlink -f "$0")")"

TITLE="File Picker"
MODE="files"
MIME_LIST=""

[ $debug = 1 ] && echo "launcher args: $*" >> $logfile
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
      echo "Invalid option: -$opt $OPTARG" >> $logfile
      exit 1
      ;;
  esac
done

if [[ ! "$MODE" =~ ^(file|files|dir|save)$ ]]; then
  echo "Error: Invalid mode flag value (-m). It should be one of [file files dir save]." >> $logfile
  exit 1
fi
if [ ! -f "$venv/bin/activate" ]; then
cat << EOF > /dev/stderr
You may need to set up a venv. Put a venv in this directory or set environment variable PK_VENV to /path/to/venv.
Try this:
python -m venv $venv
. $venv/bin/activate
pip3 install -r requirements.txt
EOF
else
	[ $debug = 1 ] && echo ". $venv/bin/activate" >> $logfile
	. $venv/bin/activate
fi

cmd="python ./pikeru.py \
	--mode '${MODE}' \
	--path '${DIR}' \
	--title '${TITLE}' \
	--mime_list '${MIME_LIST[@]:-}' \
	--parent '${PARENT:-}'" 
[ $debug = 1 ] && echo "cmd: $cmd" >> $logfile
eval "$cmd"
