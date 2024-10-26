"use strict";
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
var __generator = (this && this.__generator) || function (thisArg, body) {
    var _ = { label: 0, sent: function() { if (t[0] & 1) throw t[1]; return t[1]; }, trys: [], ops: [] }, f, y, t, g = Object.create((typeof Iterator === "function" ? Iterator : Object).prototype);
    return g.next = verb(0), g["throw"] = verb(1), g["return"] = verb(2), typeof Symbol === "function" && (g[Symbol.iterator] = function() { return this; }), g;
    function verb(n) { return function (v) { return step([n, v]); }; }
    function step(op) {
        if (f) throw new TypeError("Generator is already executing.");
        while (g && (g = 0, op[0] && (_ = 0)), _) try {
            if (f = 1, y && (t = op[0] & 2 ? y["return"] : op[0] ? y["throw"] || ((t = y["return"]) && t.call(y), 0) : y.next) && !(t = t.call(y, op[1])).done) return t;
            if (y = 0, t) op = [op[0] & 2, t.value];
            switch (op[0]) {
                case 0: case 1: t = op; break;
                case 4: _.label++; return { value: op[1], done: false };
                case 5: _.label++; y = op[1]; op = [0]; continue;
                case 7: op = _.ops.pop(); _.trys.pop(); continue;
                default:
                    if (!(t = _.trys, t = t.length > 0 && t[t.length - 1]) && (op[0] === 6 || op[0] === 2)) { _ = 0; continue; }
                    if (op[0] === 3 && (!t || (op[1] > t[0] && op[1] < t[3]))) { _.label = op[1]; break; }
                    if (op[0] === 6 && _.label < t[1]) { _.label = t[1]; t = op; break; }
                    if (t && _.label < t[2]) { _.label = t[2]; _.ops.push(op); break; }
                    if (t[2]) _.ops.pop();
                    _.trys.pop(); continue;
            }
            op = body.call(thisArg, _);
        } catch (e) { op = [6, e]; y = 0; } finally { f = t = 0; }
        if (op[0] & 5) throw op[1]; return { value: op[0] ? op[1] : void 0, done: true };
    }
};
Object.defineProperty(exports, "__esModule", { value: true });
var fs = require("fs");
var os = require("os");
var path = require("path");
var babel = require("@babel/core");
// @ts-expect-error
var envPreset = require("@babel/preset-env");
// @ts-expect-error
var tsPreset = require("@babel/preset-typescript");
var core_1 = require("@swc-node/core");
var benchmark_1 = require("benchmark");
var colorette_1 = require("colorette");
var esbuild_1 = require("esbuild");
var ts = require("typescript");
var cpuCount = os.cpus().length - 1;
var syncSuite = new benchmark_1.Suite('Transform rxjs/AjaxObservable.ts benchmark');
var asyncSuite = new benchmark_1.Suite('Transform rxjs/AjaxObservable.ts async benchmark');
var parallelSuite = new benchmark_1.Suite('Transform rxjs/AjaxObservable.ts parallel benchmark');
var SOURCE_PATH = path.join(__dirname, '..', 'node_modules', 'rxjs', 'src', 'internal', 'ajax', 'ajax.ts');
var SOURCE_CODE = fs.readFileSync(SOURCE_PATH, 'utf-8');
function run() {
    return __awaiter(this, void 0, void 0, function () {
        var defer, task;
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0:
                    task = new Promise(function (resolve) {
                        defer = resolve;
                    });
                    syncSuite
                        .add('esbuild', function () {
                        (0, esbuild_1.transformSync)(SOURCE_CODE, {
                            sourcefile: SOURCE_PATH,
                            loader: 'ts',
                            sourcemap: true,
                            minify: false,
                            target: 'es2015',
                        });
                    })
                        .add('@swc-node/core', function () {
                        (0, core_1.transformSync)(SOURCE_CODE, SOURCE_PATH, {
                            // SWC target es2016 is es2015 in TypeScript
                            target: 'es2016',
                            module: 'commonjs',
                            sourcemap: true,
                        });
                    })
                        .add('typescript', function () {
                        ts.transpileModule(SOURCE_CODE, {
                            fileName: SOURCE_PATH,
                            compilerOptions: {
                                target: ts.ScriptTarget.ES2015,
                                module: ts.ModuleKind.CommonJS,
                                isolatedModules: true,
                                sourceMap: true,
                            },
                        });
                    })
                        .add('babel', function () {
                        babel.transform(SOURCE_CODE, {
                            filename: SOURCE_PATH,
                            presets: [
                                tsPreset,
                                [envPreset, { useBuiltIns: false, loose: true, targets: 'Chrome > 52', modules: 'commonjs' }],
                            ],
                            configFile: false,
                            babelrc: false,
                            sourceMaps: true,
                        });
                    })
                        .on('cycle', function (event) {
                        console.info(String(event.target));
                    })
                        .on('complete', function () {
                        console.info("".concat(this.name, " bench suite: Fastest is ").concat((0, colorette_1.green)(this.filter('fastest')
                            .map(function (s) { return s.name; })
                            .join(''))));
                        defer();
                    })
                        .run();
                    return [4 /*yield*/, task];
                case 1:
                    _a.sent();
                    return [2 /*return*/];
            }
        });
    });
}
function runAsync() {
    return __awaiter(this, arguments, void 0, function (parallel, suite) {
        var defer, task;
        if (parallel === void 0) { parallel = 1; }
        if (suite === void 0) { suite = asyncSuite; }
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0:
                    task = new Promise(function (resolve) {
                        defer = resolve;
                    });
                    suite
                        .add({
                        name: '@swc-node/core',
                        fn: function (deferred) {
                            Promise.all(Array.from({ length: parallel }).map(function () {
                                return (0, core_1.transform)(SOURCE_CODE, SOURCE_PATH, {
                                    target: 'es2016',
                                    module: 'commonjs',
                                    sourcemap: true,
                                });
                            }))
                                .then(function () {
                                deferred.resolve();
                            })
                                .catch(function (e) {
                                console.error(e);
                            });
                        },
                        defer: true,
                        async: true,
                        queued: true,
                    })
                        .add({
                        name: 'esbuild',
                        fn: function (deferred) {
                            Promise.all(Array.from({ length: parallel }).map(function () {
                                return (0, esbuild_1.transform)(SOURCE_CODE, {
                                    sourcefile: SOURCE_PATH,
                                    loader: 'ts',
                                    sourcemap: true,
                                    minify: false,
                                    target: 'es2015',
                                });
                            }))
                                .then(function () {
                                deferred.resolve();
                            })
                                .catch(function (e) {
                                console.error(e);
                            });
                        },
                        defer: true,
                        async: true,
                        queued: true,
                    })
                        .on('cycle', function (event) {
                        event.target.hz = event.target.hz * parallel;
                        console.info(String(event.target));
                    })
                        .on('complete', function () {
                        console.info("".concat(this.name, " bench suite: Fastest is ").concat((0, colorette_1.green)(this.filter('fastest')
                            .map(function (t) { return t.name; })
                            .join(''))));
                        defer();
                    })
                        .run();
                    return [4 /*yield*/, task];
                case 1:
                    _a.sent();
                    return [2 /*return*/];
            }
        });
    });
}
run()
    .then(function () { return runAsync(cpuCount, parallelSuite); })
    .catch(console.error);
