#!/usr/bin/env bash

# Отримання поточної директорії скрипта
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Перезапуск у nix-shell для NixOS
if [ -f /etc/NIXOS ] && [ -z "$MC_SERVER_GUI_NIX" ]; then
    echo "❄️ NixOS detected. Running installer inside nix-shell..."
    export MC_SERVER_GUI_NIX=1
    exec nix-shell "$SCRIPT_DIR/shell.nix" --run "cd \"$SCRIPT_DIR\" && ./install.sh"
fi

# Шляхи (використовуємо абсолютні для стабільності)
REAL_HOME=$(eval echo "~$USER")
INSTALL_DIR="$REAL_HOME/.local/share/minecraft-server-manager"
REPO_DIR="$INSTALL_DIR/source"
BIN_DIR="$REAL_HOME/.local/bin"
LAUNCHER="$BIN_DIR/mc_control_panel"
BINARY_ENGINE="$INSTALL_DIR/mc_control_panel-bin"
DESKTOP_PATH="$REAL_HOME/.local/share/applications/mc_control_panel.desktop"
GITHUB_REPO="https://github.com/zoozieuniver/minecraft_server_manager.git"

echo "🚀 Початок встановлення Minecraft Server Manager..."

# 1. Створення необхідних папок
mkdir -p "$INSTALL_DIR" "$BIN_DIR" "$REAL_HOME/.local/share/applications"

# 2. Автовизначення ОС та встановлення залежностей
OS_TYPE="unknown"
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS_TYPE=$ID
    OS_LIKE=$ID_LIKE
fi

echo "🔍 Виявлено операційну систему: $OS_TYPE"

# Функція перевірки та встановлення Rust/Cargo
install_rust_if_missing() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "🦀 Rust/Cargo не знайдено. Встановлення..."
        if [ "$OS_TYPE" = "arch" ] || [ "$OS_LIKE" = "arch" ]; then
            sudo pacman -S --noconfirm rust
        elif [ "$OS_TYPE" = "fedora" ]; then
            sudo dnf install -y cargo rust
        elif [ "$OS_TYPE" = "debian" ] || [ "$OS_TYPE" = "ubuntu" ] || [ "$OS_LIKE" = "debian" ]; then
            sudo apt-get update && sudo apt-get install -y cargo rustc
        elif [ "$OS_TYPE" = "gentoo" ]; then
            if command -v sudo >/dev/null 2>&1; then
                sudo emerge -n dev-lang/rust
            else
                su -c "emerge -n dev-lang/rust"
            fi
        else
            echo "📥 Встановлення Rust через rustup..."
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
            source "$REAL_HOME/.cargo/env"
        fi
    fi
}

case "$OS_TYPE" in
    nixos)
        echo "❄️ Знайдено NixOS. Використовуємо nix-shell для збірки (права root не потрібні)."
        ;;
    arch|cachyos|endeavouros)
        echo "📦 Встановлення залежностей через pacman..."
        sudo pacman -S --noconfirm git pkgconf openssl gtk3 libx11 libxkbcommon desktop-file-utils
        install_rust_if_missing
        ;;
    fedora)
        echo "📦 Встановлення залежностей через dnf..."
        sudo dnf install -y git pkgconfig openssl-devel gtk3-devel libX11-devel libxkbcommon-devel desktop-file-utils
        install_rust_if_missing
        ;;
    debian|ubuntu|pop|mint)
        echo "📦 Встановлення залежностей через apt..."
        sudo apt-get update
        sudo apt-get install -y git pkg-config libssl-dev libgtk-3-dev libx11-dev libxkbcommon-dev desktop-file-utils
        install_rust_if_missing
        ;;
    gentoo)
        echo "📦 Встановлення залежностей через emerge..."
        if command -v sudo >/dev/null 2>&1; then
            sudo emerge -n dev-vcs/git dev-util/pkgconfig dev-libs/openssl x11-libs/gtk+:3 x11-libs/libX11 x11-libs/libxkbcommon dev-util/desktop-file-utils
        else
            echo "🔑 Потрібні права root для встановлення залежностей (використовуємо su):"
            su -c "emerge -n dev-vcs/git dev-util/pkgconfig dev-libs/openssl x11-libs/gtk+:3 x11-libs/libX11 x11-libs/libxkbcommon dev-util/desktop-file-utils"
        fi
        install_rust_if_missing
        ;;
    *)
        # Намагаємося вгадати за менеджером пакетів
        if command -v pacman >/dev/null 2>&1; then
            sudo pacman -S --noconfirm git pkgconf openssl gtk3 libx11 libxkbcommon desktop-file-utils
            install_rust_if_missing
        elif command -v dnf >/dev/null 2>&1; then
            sudo dnf install -y git pkgconfig openssl-devel gtk3-devel libX11-devel libxkbcommon-devel desktop-file-utils
            install_rust_if_missing
        elif command -v apt-get >/dev/null 2>&1; then
            sudo apt-get update && sudo apt-get install -y git pkg-config libssl-dev libgtk-3-dev libx11-dev libxkbcommon-dev desktop-file-utils
            install_rust_if_missing
        elif command -v emerge >/dev/null 2>&1; then
            su -c "emerge -n dev-vcs/git dev-util/pkgconfig dev-libs/openssl x11-libs/gtk+:3 x11-libs/libX11 x11-libs/libxkbcommon dev-util/desktop-file-utils"
            install_rust_if_missing
        else
            echo "⚠️ Не вдалося визначити менеджер пакетів вашої ОС. Перевірте, чи встановлено git, pkg-config, openssl, gtk3, libx11, libxkbcommon."
        fi
        ;;
esac

# 3. Підготовка репозиторію оновлень ($REPO_DIR)
if [ -d "$REPO_DIR/.git" ]; then
    echo "🔄 Оновлення кешу вихідного коду в $REPO_DIR..."
    cd "$REPO_DIR" && git fetch --all >/dev/null 2>&1 && git reset --hard origin/main >/dev/null 2>&1 && git clean -fd >/dev/null 2>&1
else
    echo "📥 Клонування репозиторію у $REPO_DIR..."
    rm -rf "$REPO_DIR"
    git clone "$GITHUB_REPO" "$REPO_DIR"
fi

# 4. Компіляція
# Якщо скрипт запущено з папки розробника (де є Cargo.toml), збираємо локальну версію.
# Якщо ні (встановлення з чистого скрипта), збираємо з папки оновлень $REPO_DIR.
BUILD_PATH=""
if [ -f "Cargo.toml" ]; then
    echo "🛠️ Збірка з локальної папки розробника..."
    BUILD_PATH=$(pwd)
else
    echo "🛠️ Збірка з папки оновлень репозиторію..."
    BUILD_PATH="$REPO_DIR"
fi

cd "$BUILD_PATH"

if [ "$OS_TYPE" = "nixos" ] || [ -f /etc/NIXOS ]; then
    if [ -n "$MC_SERVER_GUI_NIX" ]; then
        cargo build --release
    else
        nix-shell "$BUILD_PATH/shell.nix" --run "cargo build --release"
    fi
else
    # Додаємо шлях до вантажу Cargo, якщо встановили через rustup
    [ -f "$REAL_HOME/.cargo/env" ] && source "$REAL_HOME/.cargo/env"
    cargo build --release
fi

if [ $? -eq 0 ]; then
    echo "✅ Програму успішно скомпільовано!"
    cp "$BUILD_PATH/target/release/mc_server_gui" "$BINARY_ENGINE"
    cp "$BUILD_PATH/shell.nix" "$INSTALL_DIR/shell.nix"
else
    echo "❌ Помилка компіляції!" && exit 1
fi

# 5. Створення лаунчера з підтримкою автооновлення
cat << 'EOF' > "$LAUNCHER"
#!/usr/bin/env bash

INSTALL_DIR="$HOME/.local/share/minecraft-server-manager"
REPO_DIR="$INSTALL_DIR/source"
EXE_PATH="$INSTALL_DIR/mc_control_panel-bin"

# Визначення ОС для запуску в NixOS
OS_TYPE="unknown"
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS_TYPE=$ID
fi

# Перевірка оновлень
if [ -d "$REPO_DIR" ]; then
    cd "$REPO_DIR" || exit
    
    echo "🔍 Перевірка наявності оновлень на GitHub..."
    git fetch >/dev/null 2>&1
    
    LOCAL=$(git rev-parse HEAD 2>/dev/null)
    REMOTE=$(git rev-parse origin/main 2>/dev/null)
    
    if [ -n "$LOCAL" ] && [ -n "$REMOTE" ] && [ "$LOCAL" != "$REMOTE" ]; then
        echo -e "\e[33m📥 Знайдено нову версію! Завантаження змін...\e[0m"
        git pull
        
        echo -e "\e[36m⚙️ Перекомпіляція програми після оновлення...\e[0m"
        rm -f "$EXE_PATH"
        
        if [ "$OS_TYPE" = "nixos" ] || [ -f /etc/NIXOS ]; then
            nix-shell "$INSTALL_DIR/shell.nix" --run "cargo build --release"
        else
            [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
            cargo build --release
        fi
        
        if [ $? -eq 0 ]; then
            cp "$REPO_DIR/target/release/mc_server_gui" "$EXE_PATH"
            echo -e "\e[32m✅ Оновлення успішно встановлено!\e[0m"
            echo -e "\e[35m👉 Натисніть Enter для запуску нової версії...\e[0m"
            read
            exec "$0" "$@"
        else
            echo -e "\e[31m❌ Помилка компіляції оновлення. Запуск поточної версії...\e[0m"
            sleep 2
        fi
    fi
fi

# Запуск бінарного файлу
if [ -f "$EXE_PATH" ]; then
    # На NixOS запускаємо через nix-shell для правильного завантаження динамічних бібліотек
    if [ "$OS_TYPE" = "nixos" ] || [ -f /etc/NIXOS ]; then
        exec nix-shell "$INSTALL_DIR/shell.nix" --run "$EXE_PATH"
    else
        exec "$EXE_PATH" "$@"
    fi
else
    echo "❌ Помилка: Виконуваний файл не знайдено за шляхом $EXE_PATH"
    exit 1
fi
EOF

chmod +x "$LAUNCHER"
[ -f "$BINARY_ENGINE" ] && chmod +x "$BINARY_ENGINE"

# 6. Створення .desktop ярлика
cat <<EOF > "$DESKTOP_PATH"
[Desktop Entry]
Version=1.0
Type=Application
Name=Minecraft Server Manager
Comment=Графічний інтерфейс для керування серверами Minecraft
Exec=$LAUNCHER
Icon=games-config
Terminal=true
Categories=Game;Utility;
EOF

chmod +x "$DESKTOP_PATH"
update-desktop-database "$REAL_HOME/.local/share/applications/" >/dev/null 2>&1

echo "✨ Встановлення успішно завершено!"
echo "👉 Спробуйте запустити 'mc_control_panel' у терміналі або знайдіть Minecraft Server Manager у меню додатків."
