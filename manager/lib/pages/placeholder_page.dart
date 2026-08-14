import 'package:fluent_ui/fluent_ui.dart';

/// Simple centered placeholder used by milestones not yet implemented.
class PlaceholderPage extends StatelessWidget {
  const PlaceholderPage({super.key, required this.title, required this.message});

  final String title;
  final String message;

  @override
  Widget build(BuildContext context) {
    return ScaffoldPage(
      header: PageHeader(title: Text(title)),
      content: Center(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          const Icon(FluentIcons.construction_cone, size: 42),
          const SizedBox(height: 16),
          Text(message, style: FluentTheme.of(context).typography.body),
        ]),
      ),
    );
  }
}
