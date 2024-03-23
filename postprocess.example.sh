#!/bin/bash
# copy/rename this file to "postprocess.sh" to enable automatic file conversions for uploads.
# postprocess.sh must read lines of files from stdin and write lines of files to stdout.
# $POSTPROCESS_DIR is exported by xdg portal to avoid using it as the starting dir next time you
#   open a file. It can be set in the config file mentioned in the man page for
#   xdg-desktop-portal-pikeru.
# This script needs to make the $POSTPROCESS_DIR directory if it doesn't exist already.

dir=${POSTPROCESS_DIR:-/tmp/pk_postprocess}
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
