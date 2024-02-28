#!/bin/bash

# comment out or set to your venv
. ~/stuff/v1/bin/activate


TITLE="File Picker"
MODE="file"
DIR="$(pwd)"
MIME_LIST=""

while getopts "e:t:k:p:i:" opt; do
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
      echo "Invalid option: -$OPTARG" >&2
      exit 1
      ;;
  esac
done

if [[ ! "$MODE" =~ ^(file|files|dir|save)$ ]]; then
  echo "Error: Invalid mode flag value (-m). It should be one of [file files dir save]." >&2
  exit 1
fi

python ./pikeru.py \
  --title "${TITLE}" \
  --mode "${MODE}" \
  --path "${DIR}" \
  --mime_list "${MIME_LIST[@]:-}" \
  --parent "${PARENT:-}"
