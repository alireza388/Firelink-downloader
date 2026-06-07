with open("Sources/Firelink/DownloadTable.swift", "r") as f:
    content = f.read()

old_column = '''                TableColumn("File Name", value: \\.fileName) { item in
                    doubleClickableCell(for: item) {
                        HStack(alignment: .top, spacing: 8) {
                            Image(systemName: item.category.symbolName)
                                .font(.title3)
                                .foregroundStyle(categoryColor(for: item.category))
                                .frame(width: 22)
                            Text(item.fileName)
                                .font(.headline)
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .allowsHitTesting(false)
                        }
                        .draggable(item.id.uuidString)
                    }
                }'''

new_column = '''                TableColumn("File Name", value: \\.fileName) { item in
                    HStack(alignment: .top, spacing: 8) {
                        Image(systemName: item.category.symbolName)
                            .font(.title3)
                            .foregroundStyle(categoryColor(for: item.category))
                            .frame(width: 22)
                        Text(item.fileName)
                            .font(.headline)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .allowsHitTesting(false)
                    }
                    .draggable(item.id.uuidString)
                }'''

content = content.replace(old_column, new_column)

import re

# Remove doubleClickableCell function block
pattern = r'\s*private func doubleClickableCell<Content: View>\(for item: DownloadItem, @ViewBuilder content: \(\) -> Content\) -> some View \{\s*content\(\)\s*\.contentShape\(Rectangle\(\)\)\s*\.simultaneousGesture\(TapGesture\(count: 2\)\.onEnded \{\s*performPrimaryAction\(for: item\)\s*\}\)\s*\}'
content = re.sub(pattern, '', content)

with open("Sources/Firelink/DownloadTable.swift", "w") as f:
    f.write(content)
