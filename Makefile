.PHONY: build app run clean

build:
	swift build -c release

app:
	Scripts/create_app_bundle.sh

run:
	swift run Firelink

clean:
	swift package clean
	rm -rf build
