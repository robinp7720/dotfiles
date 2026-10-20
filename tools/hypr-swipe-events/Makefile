PLUGIN := hypr-swipe-events.so
PLUGIN_DIR ?= $(HOME)/.local/share/hyprland/plugins

CXXFLAGS ?= -O2 -g
CXXFLAGS += -shared -fPIC --no-gnu-unique -std=c++2b -Wall -Wextra -Wno-unused-parameter
PKGS := pixman-1 libdrm hyprland pangocairo libinput libudev wayland-server xkbcommon

all: $(PLUGIN)

$(PLUGIN): src/main.cpp
	$(CXX) $(CXXFLAGS) $< -o $@ $$(pkg-config --cflags $(PKGS))

install: $(PLUGIN)
	install -Dm755 $(PLUGIN) $(PLUGIN_DIR)/$(PLUGIN)

clean:
	rm -f $(PLUGIN)

.PHONY: all install clean
