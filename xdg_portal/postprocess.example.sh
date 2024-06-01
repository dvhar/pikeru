#!/bin/bash
# point "postprocessor" in the xdg portal config to this file to enable automatic file conversions for uploads.
# Postprocessor script must read lines of files from stdin and write lines of files to stdout.
# POSTPROCESS_DIR is exported by the portal that invokes this script. Use it for output files if you don't want
# the filepicker to use your temporary directory as the next starting location.

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
