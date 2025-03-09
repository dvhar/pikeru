#!/bin/bash
# This wrapper script is invoked by xdg-desktop-portal-pikeru.
#
# Inputs:
# 1. "1" if multiple files can be chosen, "0" otherwise.
# 2. "1" if a directory should be chosen, "0" otherwise.
# 3. "0" if opening files was requested, "1" if writing to a file was
#    requested. For example, when uploading files in Firefox, this will be "0".
#    When saving a web page in Firefox, this will be "1".
# 4. If writing to a file, this is recommended path provided by the caller. For
#    example, when saving a web page in Firefox, this will be the recommended
#    path Firefox provided, such as "~/Downloads/webpage_title.html".
#    Note that if the path already exists, we keep appending "_" to it until we
#    get a path that does not exist.
# 5. The output path, to which results should be written.
#
# Output:
# The script should print the selected paths to the output path (argument #5),
# one path per line.
# If nothing is printed, then the operation is assumed to have been canceled.
#
# Notes:
# Chrome doesn't provide the previous path via portal so need to do that here.
# Mime filters not yet implemented in this xdg portal backend.

multiple="$1"
directory="$2"
save="$3"
path="$4"

[ -z "$path" ] && path="$HOME"

#echo "'$1' '$2' '$3' '$path' '$5'" >> /tmp/pk.log

if [ $directory = 1 ]; then
    mode=dir
elif [ $multiple = 1 ]; then
    mode=files
elif [ $save = 1 ]; then
    mode=save
else
    mode=file
fi


#pikerudir="$(dirname "$(readlink -f "$0")")"
#cmd="$pikerudir/../target/release/pikeru -m $mode -t 'File Picker' -p \"$path\""

cmd="pikeru -m $mode -t 'File Picker' -p \"$path\""

# iced has a problem with crashing when no gpu is available so disable and retry if that happens
[ -r "$HOME/.cache/pikeru/no_gpu" ] && export ICED_BACKEND=tiny-skia
echo "$cmd" >> /tmp/pk.log
output="$(eval "$cmd")"
if [ $? = 139 ] && [ ! -r "$HOME/.cache/pikeru/no_gpu" ]; then
    echo "Iced GUI gpu library crashed. Fixing now..." >> /tmp/pk.log
    touch "$HOME/.cache/pikeru/no_gpu"
    export ICED_BACKEND=tiny-skia
    output="$(eval "$cmd")"
fi

if [ ! -z "$POSTPROCESSOR" ] && [ -r "$POSTPROCESSOR" ] && [ ! -z "$POSTPROCESS_DIR" ]; then
    mkdir -p "$POSTPROCESS_DIR"
    echo "$output" | bash "$POSTPROCESSOR"
else
    echo "$output"
fi

