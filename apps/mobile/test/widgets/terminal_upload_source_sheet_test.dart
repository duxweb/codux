import 'package:codux_flutter/services/terminal_upload_metadata.dart';
import 'package:codux_flutter/widgets/components/terminal_upload_source_sheet.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('returns selected upload source', (tester) async {
    TerminalUploadSource? selected;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () async {
              selected = await showModalBottomSheet<TerminalUploadSource>(
                context: context,
                builder: (_) => const TerminalUploadSourceSheet(
                  fileLabel: 'File',
                  imageLabel: 'Image',
                ),
              );
            },
            child: const Text('Open'),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Image'));
    await tester.pumpAndSettle();

    expect(selected, TerminalUploadSource.image);
  });
}
