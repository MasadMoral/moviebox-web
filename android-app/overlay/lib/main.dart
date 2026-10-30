import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';
import 'package:path_provider/path_provider.dart';

const int _port = 8787;
const MethodChannel _native = MethodChannel('mbw/native');

typedef _StartC = Int32 Function(Uint16, Pointer<Utf8>);
typedef _StartDart = int Function(int, Pointer<Utf8>);

/// Starts the embedded Rust server (libmoviebox_web.so) inside this app process.
Future<void> _startServer() async {
  final dir = await getApplicationSupportDirectory();
  final lib = DynamicLibrary.open('libmoviebox_web.so');
  final start = lib.lookupFunction<_StartC, _StartDart>('mbw_start');
  final path = dir.path.toNativeUtf8();
  start(_port, path);
  malloc.free(path);
}

Future<void> _waitReady() async {
  final client = HttpClient()..connectionTimeout = const Duration(seconds: 2);
  for (var i = 0; i < 80; i++) {
    try {
      final req = await client.getUrl(Uri.parse('http://127.0.0.1:$_port/'));
      final res = await req.close();
      await res.drain();
      if (res.statusCode == 200) {
        client.close();
        return;
      }
    } catch (_) {}
    await Future.delayed(const Duration(milliseconds: 300));
  }
  client.close();
  throw Exception('The embedded server did not start');
}

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const MovieBoxApp());
}

class MovieBoxApp extends StatelessWidget {
  const MovieBoxApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'MovieBox Web',
      debugShowCheckedModeBanner: false,
      theme: ThemeData.dark(useMaterial3: true).copyWith(
        scaffoldBackgroundColor: const Color(0xFF0F1115),
      ),
      home: const Home(),
    );
  }
}

class Home extends StatefulWidget {
  const Home({super.key});

  @override
  State<Home> createState() => _HomeState();
}

class _HomeState extends State<Home> {
  InAppWebViewController? _web;
  bool _ready = false;
  String? _error;
  final List<String> _pendingJs = [];

  @override
  void initState() {
    super.initState();
    _native.setMethodCallHandler(_onNativeCall);
    _boot();
  }

  /// Commands coming from the Android side (Bluetooth/media keys, PiP changes).
  Future<dynamic> _onNativeCall(MethodCall call) async {
    switch (call.method) {
      case 'mediaCommand':
        final cmd = (call.arguments as String? ?? '')
            .replaceAll('\\', r'\\')
            .replaceAll("'", r"\'");
        _eval("window.mbwMediaCommand&&window.mbwMediaCommand('$cmd')");
        break;
      case 'pipChanged':
        _eval('window.mbwPip&&window.mbwPip(${call.arguments == true})');
        break;
    }
    return null;
  }

  void _eval(String js) {
    final c = _web;
    if (c == null) {
      _pendingJs.add(js);
      return;
    }
    c.evaluateJavascript(source: js);
  }

  double _num(dynamic v, [double fallback = 0]) => v is num ? v.toDouble() : fallback;

  /// Calls from the page: brightness, volume, PiP, media state.
  Future<dynamic> _onJs(List<dynamic> args) async {
    final data = (args.isNotEmpty && args.first is Map)
        ? Map<String, dynamic>.from(args.first as Map)
        : <String, dynamic>{};
    final method = data['method'] as String? ?? '';
    final a = (data['args'] as List?) ?? const [];
    switch (method) {
      case 'getVolume':
        return await _native.invokeMethod<double>('getVolume');
      case 'setVolume':
        await _native.invokeMethod(
            'setVolume', a.isEmpty ? 1.0 : _num(a.first, 1.0));
        return true;
      case 'getBrightness':
        return await _native.invokeMethod<double>('getBrightness');
      case 'setBrightness':
        await _native.invokeMethod(
            'setBrightness', a.isEmpty ? 0.75 : _num(a.first, 0.75));
        return true;
      case 'enterPip':
        return await _native.invokeMethod<bool>('enterPip');
      case 'mediaState':
        final m = (a.isNotEmpty && a.first is Map)
            ? Map<String, dynamic>.from(a.first as Map)
            : <String, dynamic>{};
        await _native.invokeMethod('mediaState', m);
        return true;
      case 'toast':
        await _native.invokeMethod('toast', a.isEmpty ? '' : '${a.first}');
        return true;
      case 'openUrl':
        await _native.invokeMethod('openUrl', a.isEmpty ? '' : '${a.first}');
        return true;
    }
    return null;
  }

  Future<void> _boot() async {
    setState(() {
      _error = null;
      _ready = false;
    });
    try {
      await _startServer();
      await _waitReady();
      if (mounted) setState(() => _ready = true);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  String _intentToHttp(String url) {
    final s = url.replaceFirst('intent://', 'http://');
    final i = s.indexOf('#Intent');
    return i >= 0 ? s.substring(0, i) : s;
  }

  @override
  Widget build(BuildContext context) {
    if (_error != null) {
      return Scaffold(
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(_error!, textAlign: TextAlign.center),
                const SizedBox(height: 16),
                FilledButton(onPressed: _boot, child: const Text('Retry')),
              ],
            ),
          ),
        ),
      );
    }
    if (!_ready) {
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) async {
        if (didPop) return;
        final c = _web;
        if (c != null && await c.canGoBack()) {
          c.goBack();
        } else {
          SystemNavigator.pop();
        }
      },
      child: Scaffold(
        body: SafeArea(
          child: InAppWebView(
            initialUrlRequest:
                URLRequest(url: WebUri('http://127.0.0.1:$_port/')),
            initialSettings: InAppWebViewSettings(
              javaScriptEnabled: true,
              mediaPlaybackRequiresUserGesture: false,
              allowsInlineMediaPlayback: true,
              useShouldOverrideUrlLoading: true,
              useOnDownloadStart: true,
              supportZoom: false,
              // Keep scrolling on the WebView's own compositor and avoid
              // drawing scrollbars/overscroll effects over the page.
              useHybridComposition: true,
              overScrollMode: OverScrollMode.NEVER,
              verticalScrollBarEnabled: false,
              horizontalScrollBarEnabled: false,
              cacheEnabled: true,
            ),
            onWebViewCreated: (c) {
              _web = c;
              c.addJavaScriptHandler(
                handlerName: 'mbwNative',
                callback: _onJs,
              );
              for (final js in _pendingJs) {
                c.evaluateJavascript(source: js);
              }
              _pendingJs.clear();
            },
            shouldOverrideUrlLoading: (c, action) async {
              final url = action.request.url?.toString() ?? '';
              if (url.startsWith('intent://')) {
                // "External player" button: hand the stream to VLC / MX Player
                await _native.invokeMethod(
                    'openExternal', {'url': _intentToHttp(url)});
                return NavigationActionPolicy.CANCEL;
              }
              return NavigationActionPolicy.ALLOW;
            },
            onDownloadStartRequest: (c, req) async {
              await _native.invokeMethod('download', {
                'url': req.url.toString(),
                'name': req.suggestedFilename ?? 'video.mp4',
              });
            },
            onEnterFullscreen: (c) async {
              SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky);
              await SystemChrome.setPreferredOrientations([
                DeviceOrientation.landscapeLeft,
                DeviceOrientation.landscapeRight,
              ]);
              // Force SENSOR_LANDSCAPE so the video flips 180° with the phone.
              try {
                await _native.invokeMethod('setFullscreen', true);
              } catch (_) {}
            },
            onExitFullscreen: (c) async {
              SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
              try {
                await _native.invokeMethod('setFullscreen', false);
              } catch (_) {}
              await SystemChrome.setPreferredOrientations(DeviceOrientation.values);
            },
          ),
        ),
      ),
    );
  }
}
