This is the old way to use it before figuring out the kdialog hack. Build chromium from source after applying the patch:
```
git apply < /path/to/chromium.patch
```
Then to use it set the environment variable `CUSTOM_FILEPICKER=/path/to/this/program/run.sh`.

## License
The Chromium patch is licensed under the same BSD-style licence as chromium because it is derived from chromium source code.
