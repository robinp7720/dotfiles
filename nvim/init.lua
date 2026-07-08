require("config.options")
require("config.keymaps")
require("config.autocmds")
require("config.lazy")

pcall(require, "config.local")
