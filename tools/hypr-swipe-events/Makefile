PLUGIN := hypr-swipe-events.so
PLUGIN_DIR ?= $(HOME)/.local/share/hyprland/plugins

CXXFLAGS ?= -O2 -g
CXXFLAGS += -shared -fPIC --no-gnu-unique -std=c++2b -Wall -Wextra -Wno-unused-parameter
PKGS := pixman-1 libdrm hyprland pangocairo libinput libudev wayland-server xkbcommon

all: $(PLUGIN)

$(PLUGIN): src/main.cpp FORCE
	@rm -f $@.abi
	@python3 ../../scripts/hyprland_plugin_abi.py > $@.abi.tmp
	$(CXX) $(CXXFLAGS) $< -o $@ $$(pkg-config --cflags $(PKGS))
	mv $@.abi.tmp $@.abi

install: $(PLUGIN)
	rm -f $(PLUGIN_DIR)/$(PLUGIN).abi
	install -Dm755 $(PLUGIN) $(PLUGIN_DIR)/$(PLUGIN)
	install -m644 $(PLUGIN).abi $(PLUGIN_DIR)/$(PLUGIN).abi

clean:
	rm -f $(PLUGIN) $(PLUGIN).abi $(PLUGIN).abi.tmp

FORCE:

.PHONY: all install clean FORCE
