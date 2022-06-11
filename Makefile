ipynb_files = $(shell find . -name '*.ipynb' -not -path '*/.ipynb_checkpoints/*')
ipynb_html_files = $(addprefix build/,$(ipynb_files:.ipynb=.html))

all: $(ipynb_html_files)
	echo $(ipynb_html_files)
	cd system && cargo build
	./system/target/debug/system

build/%.html: %.ipynb
	jupyter nbconvert --to html $^ --stdout > $@
