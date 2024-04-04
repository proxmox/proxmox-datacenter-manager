
all:

# bash completion
%.bc:
	@echo "creating bash completion definition for $*"
	@printf '# $* bash completion\n\n' >$@.tmp
	@printf "complete -C '$* bashcomplete' $*\n" >>$@.tmp
	mv $@.tmp $@

# ZSH completion
_%:
	@echo "creating ZSH completion definition for $*"
	@printf '#compdef _$*() $*\n\n' >$@.tmp
	@printf 'function _$*() {\n' >>$@.tmp
	@printf '    local cwords line point cmd curr prev\n' >>$@.tmp
	@printf '    cwords=$${#words[@]}\n' >>$@.tmp
	@printf '    line=$$words\n' >>$@.tmp
	@printf '    point=$${#line}\n' >>$@.tmp
	@printf '    cmd=$${words[1]}\n' >>$@.tmp
	@printf '    curr=$${words[cwords]}\n' >>$@.tmp
	@printf '    prev=$${words[cwords-1]}\n' >>$@.tmp
	@printf '    compadd -- $$(COMP_CWORD="$$cwords" COMP_LINE="$$line" COMP_POINT="$$point" \\n' >>$@.tmp
	@printf '        $* bashcomplete "$$cmd" "$$curr" "$$prev")\n' >>$@.tmp
	@printf '}\n' >>$@.tmp
	mv $@.tmp $@

clean:
	rm -f _* *.bc
