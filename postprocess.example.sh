#!/bin/bash

# /tmp/pk_postprocessed is hardcoded into the xdg portal to avoid using it as the starting dir next time you open a file
dir=/tmp/pk_postprocessed
mkdir -p $dir

while read file; do
	case "${file,,}" in
		*.webp)
			base="$(basename "$file")"
			convert "$file" "$dir/${base%.webp}.jpg"
			echo "$dir/${base%.webp}.jpg"
			;;
		*)
			echo "$file"
			;;
	esac
done
