import 'package:flutter/material.dart';

class IconTextLabel extends StatelessWidget {
  const IconTextLabel({super.key, required this.icon, required this.label});

  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [Icon(icon, size: 17), const SizedBox(width: 6), Text(label)],
    );
  }
}
