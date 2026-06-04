.PHONY: build app dmg run verify clean

build:
	swift build -c release

app:
	Scripts/create_app_bundle.sh

dmg: app
	Scripts/create_dmg.sh

run:
	swift run Firelink

verify:
	Scripts/verify.sh

clean:
	swift package clean
	rm -rf build dist
