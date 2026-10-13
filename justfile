#!/usr/bin/env just --justfile

export ANDROID_NDK_HOME := env("HOME") / "Data/Setup/Android/SDK/ndk/"
export ANDROID_SDK_ROOT := env("HOME") / "Data/Setup/Android/SDK/"
export CARGO_FEATURE_PURE := "1"

android:
    rm -rf mobile/app/src/main/jniLibs
    rm -rf mobile/app/src/main/assets
    mkdir -p mobile/app/src/main/assets
    rsync -a app/assets/ mobile/app/src/main/assets/
    cd app && cargo ndk -t arm64-v8a -p 36 -o ../mobile/app/src/main/jniLibs build --release --lib
    #cd mobile && ./gradlew assembleDebug
    #mkdir -p output
    #cp mobile/app/build/outputs/apk/debug/app-debug.apk output/rurarendar.apk

wasm:
    wasm-pack build --release --target web
