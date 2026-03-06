#!/bin/bash

sf() {
	case "$1" in
		search|coaccess)
			local dest
			dest="$(command sf "$@")"
			if [ -n "$dest" ]; then
				builtin cd "$dest" || return 1
			fi
			;;
		*)
			command sf "$@"
			;;
	esac
}
