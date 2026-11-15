# AstralLauncher

Современный лаунчер Minecraft для Windows: нативный бэкенд на Rust, интерфейс на Tauri 2 + React + TypeScript, тёмная минималистичная тема и встроенный каталог Modrinth.

## Возможности

- Установка любой официальной версии Minecraft из манифеста Mojang (релизы, снапшоты, старые версии — с бейджами «релиз» / «снапшот» / «старая»). Для каждой версии показывается дата выхода и полный размер (клиент + библиотеки + ассеты) ещё до скачивания. Установленная версия автоматически становится активной, клиентский JAR кладётся как `versions/<id>/<id>.jar`.
- Мод-лоадеры одним кликом: Vanilla, Fabric, Forge. Fabric ставится через официальное Meta API, Forge — через headless-установщик (Java подбирается по манифесту версии, для Minecraft 26.x это Java 25). Forge доступен только для релизов — на снапшотах кнопка блокируется с подсказкой.
- Встроенный каталог Modrinth (API v2): поиск, фильтры по типу контента (моды, ресурспаки, шейдеры, карты), версии игры и загрузчику; выбор конкретной версии релиза в карточке; автоматическое разрешение и скачивание зависимостей; файлы распаковываются в нужные папки (`mods`, `resourcepacks`, `shaderpacks`, `saves`).
- Агрегированный прогресс загрузок: общий вес всех файлов задачи, сколько скачано, сколько осталось, скорость, число файлов — одной строкой, без «по одному файлу». Есть отмена задачи.
- Профессиональная консоль в стиле IntelliJ / VS Code: цветовая дифференциация по уровням (info / warn / error / debug), фильтры по уровню и по тексту, автопрокрутка, копирование и экспорт лога. Логи игры парсятся из log4j-формата в реальном времени.
- Настройки: каталог установки (по умолчанию `C:\AstralLauncher`, меняется на любой путь), выделение ОЗУ (1024 МБ – 32 ГБ), параметры окна, дополнительные JVM/игровые аргументы, автоматическая установка JRE (Eclipse Adoptium, Java 8/16/17/21 по требованию версии), управление аккаунтами (офлайн и Microsoft через device code flow).
- Всё хранится в одном каталоге (по умолчанию `C:\AstralLauncher`): `versions`, `libraries`, `assets`, `runtime`, `minecraft\mods` и т.д. Игра запускается с рабочей директорией `minecraft` внутри каталога лаунчера.

## Технологии

| Слой | Стек |
| --- | --- |
| Бэкенд | Rust, Tauri 2, tokio, reqwest (native-tls/schannel), serde |
| Интерфейс | Tauri 2 WebView, React 18, TypeScript (strict), Vite 6 |
| Шрифт | Inter (@fontsource, бандлится локально) |
| Иконки | Только inline SVG, без эмодзи |
| Установщик | NSIS (Tauri bundler, per-machine, русский и английский языки) |
| CI | GitHub Actions, windows-latest, artifact + Release на теге |

## Структура проекта

```
AstralLauncher/
├── .github/workflows/build.yml      CI: сборка NSIS-установщика и релиз
├── package.json / tsconfig.json / vite.config.ts
├── index.html
├── src/                             Фронтенд (React + TS)
│   ├── main.tsx                     Точка входа, шрифты Inter
│   ├── App.tsx                      Навигация, события, панель загрузок, тосты
│   ├── api.ts                       Типизированные вызовы invoke + форматирование
│   ├── types.ts                     Типы, зеркалящие структуры бэкенда
│   ├── icons.tsx                    Библиотека SVG-иконок (28 иконок)
│   ├── styles.css                   Дизайн-система (палитра, компоненты, консоль)
│   └── components/
│       ├── PlayTab.tsx              Каталог версий (дата, размер), лоадеры, запуск
│       ├── ModsTab.tsx              Каталог Modrinth, карточки, модалка с версиями
│       ├── SettingsTab.tsx          Путь, RAM, Java, аргументы, аккаунты, MS-вход
│       ├── ConsoleTab.tsx           Логи с фильтрами, экспорт
│       └── common.tsx               Прогресс-бар, спиннер, пустые состояния
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json              Окно, NSIS (perMachine, 2 языка), иконки
    ├── capabilities/default.json    Разрешения (core, dialog)
    ├── icons/                       icon.ico + PNG (генерируются scripts/gen_icons.py)
    └── src/
        ├── main.rs                  Точка входа (windows_subsystem)
        ├── lib.rs                   Builder Tauri + все команды invoke
        ├── config.rs                Config: путь, RAM, Java, аккаунты (launcher-config.json)
        ├── state.rs                 AppState, реестр задач с отменой
        ├── http.rs                  Общий reqwest-клиент с User-Agent
        ├── util.rs                  Правила Mojang, SHA-1, распаковка zip, ID задач
        ├── download.rs              Агрегированный загрузчик (прогресс, отмена, retry, SHA-1)
        ├── version.rs               Манифест Mojang, размер версии, установка (клиент, библиотеки, ассеты, natives), слияние JSON лоадеров
        ├── loaders.rs               Fabric Meta, Forge maven + headless-установщики
        ├── modrinth.rs              Поиск, версии, зависимости, установка по типу контента
        ├── java.rs                  Поиск Java, Adoptium JRE, определение мажорной версии
        ├── accounts.rs              Офлайн-UUID (v3), Microsoft device code flow (XBL/XSTS/MC)
        ├── launch.rs                Сборка JVM/игровых аргументов, спавн процесса, стриминг логов
        └── logger.rs                Консольные события, разбор log4j-строк
```

## Архитектура

**События (Rust → UI):** `download-progress` (агрегированный прогресс), `task-finished` (завершение установки/загрузки Java), `console-log` (все логи лаунчера и игры), `game-exit` (код возврата).

**Команды (UI → Rust):** конфигурация (`get_config`, `update_config`, `set_root_path`, `get_root_info`, `open_folder`), версии (`list_versions`, `version_size`, `list_installed_versions`, `install_version_cmd`, `set_active_version`), лоадеры (`loader_versions`), Java (`install_java_cmd`, `get_java_runtimes`), аккаунты (`add_offline_account`, `ms_login_start`, `ms_login_finish_cmd`, `remove_account`, `set_active_account`), Modrinth (`modrinth_search_cmd`, `modrinth_project_cmd`, `modrinth_versions_cmd`, `modrinth_install_cmd`, `list_content_files_cmd`, `delete_content_file_cmd`), игра (`launch_game`, `stop_game_cmd`), отмена (`cancel_job`).

**Загрузчик** работает пулом из 6 параллельных соединений: перед скачиванием собирает Content-Length всех файлов (HEAD-запросы пачками), файлы, уже лежащие на диске с корректным SHA-1/размером, засчитываются в общий прогресс. Прогресс-события троттлятся до ~7 в секунду, скачивание идёт в `.part` с переименованием, три попытки на файл, отмена через AtomicBool.

**Запуск игры:** чтение эффективного JSON версии (с рекурсивным разворачиванием `inheritsFrom` для лоадеров), правила Mojang (os/rules/features), classpath, natives, log4j-конфигурация, подстановка всех плейсхолдеров (`${classpath}`, `${auth_player_name}` и др.), выбор Java по `javaVersion.majorVersion` (с фолбэком по номеру версии), автоустановка JRE при отсутствии, спавн `javaw.exe` с `CREATE_NO_WINDOW`, стриминг stdout/stderr в консоль, `taskkill /T` для остановки.

## Сборка

Требования: Node.js 18+, Rust stable, Windows.

```bash
npm install
npm run tauri build
```

Результат: `src-tauri/target/release/bundle/nsis/AstralLauncher_1.0.0_x64-setup.exe` — NSIS-установщик (per-machine, выбор языка русский/английский, встраивает WebView2 bootstrapper).

Готовый установщик собирает GitHub Actions: push в `main` или тег `v*` — workflow `.github/workflows/build.yml` ставит Node и Rust, выполняет `npm ci`, `tsc --noEmit`, `tauri build` и загружает артефакт `AstralLauncher-installer` (на теге также создаётся GitHub Release).

Локальная проверка типов без сборки: `npm run build` (tsc + vite).

## Замечания

- **Microsoft-вход** требует `client_id` приложения Azure (Настройки → Аккаунты). Создайте приложение типа «Mobile and desktop applications» с redirect URI `https://login.microsoftonline.com/common/oauth2/nativeclient` и включите публичную аутентификацию (allow public client flows). Без него работает офлайн-режим.
- **Forge** ставится официальным headless-установщиком (`--installClient`): требуемая Java определяется из `javaVersion` JSON версии Mojang (например, Java 25 для 26.x); при отсутствии лаунчер сообщит код `JAVA_MISSING:<мажор>` и предложит установить Java одним кликом.
- **Внешний вид**: размытые обои-фон на выбор (Полярное сияние / Туманность / Угольки / свои обои / без обоев), сила размытия (0–40 px) и затемнения (10–80%) регулируются слайдерами в «Настройки → Внешний вид»; свои обои (PNG/JPG/WebP/GIF до 12 МБ) копируются в каталог лаунчера. Панели поверх — полупрозрачные с backdrop-blur.
- Версии старше 1.6 (assets `legacy`) поддерживаются в базовом виде: стандартные аргументы `minecraftArguments` и legacy-индекс ассетов.
- User-Agent лаунчера содержит пометку об интеграции с Modrinth API (требование правил Modrinth).
- Проект ориентирован только на Windows (сборка и запуск): пути, `javaw.exe`, `taskkill`, NSIS.

## Лицензия

Код проекта распространяется свободно. Minecraft — торговая марка Mojang/Microsoft; лаунчер не поставляет игровые файлы, а скачивает их с официальных серверов Mojang.
