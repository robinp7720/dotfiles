# Get the directory of the script
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo "Setting up dotfiles from $DIR"

# Create symlinks for dotfiles
ln -sf "$DIR/zshrc" "$HOME/.zshrc"

directories=(
  "bspwm"
  "dunst"
  "hypr"
  "kitty"
  "matugen"
  "polybar"
  "rofi"
  "sxhkd"
  "termite"
  "waybar"
)

for dir in "${directories[@]}"; do
  echo "Linking $dir configuration"
  rm "$HOME/.config/$dir"
  ln -sf "$DIR/$dir/" "$HOME/.config/$dir"
done


