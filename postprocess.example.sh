#!/bin/bash
# postprocess.sh must read lines of files from stdin and write lines of files to stdout.
# /tmp/pk_postprocessed is hardcoded into the xdg portal to avoid using it as the starting dir next time you open a file.

dir=/tmp/pk_postprocessed
mkdir -p $dir

while read file; do
	case "${file,,}" in
		*.webp)
			base="$(basename "$file")"
			converted="$dir/${base%.webp}.jpg"
			convert "$file" "$converted"
			echo "$converted"
			;;
		*)
			echo "$file"
			;;
	esac
done
