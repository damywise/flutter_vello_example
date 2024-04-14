// The original content is temporarily commented out to allow generating a self-contained demo - feel free to uncomment later.

// import 'package:flutter/material.dart';
//
// void main() {
//   runApp(const MainApp());
// }
//
// class MainApp extends StatelessWidget {
//   const MainApp({super.key});
//
//   @override
//   Widget build(BuildContext context) {
//     return const MaterialApp(
//       home: Scaffold(
//         body: Center(
//           child: Text('Hello World!'),
//         ),
//       ),
//     );
//   }
// }
//

import 'dart:typed_data';

import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_vello/src/rust/api/simple.dart';
import 'package:flutter_vello/src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  ui.Image? image;

  bool isRendering = false;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(title: const Text('flutter_rust_bridge quickstart')),
        body: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.center,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                  'Action: Call Rust `greet("Tom")`\nResult: `${greet(name: "Tom")}`'),
              Listener(
                onPointerHover: (event) async {
                  if (isRendering) {
                    return;
                  }
                  isRendering = true;
                  final bytes = await testRender(
                    x: event.localPosition.dx,
                    y: event.localPosition.dy,
                  );
                  ui.decodeImageFromPixels(
                    bytes,
                    800,
                    600,
                    ui.PixelFormat.rgba8888,
                    (result) {
                      isRendering = false;
                      setState(() {
                        image = result;
                      });
                    },
                  );
                },
                child: Container(
                  width: 800,
                  height: 600,
                  decoration: BoxDecoration(
                    border: Border.all(
                      color: Colors.black,
                      width: 1,
                    ),
                  ),
                  child: image == null
                      ? const SizedBox.shrink()
                      : RawImage(image: image!),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
