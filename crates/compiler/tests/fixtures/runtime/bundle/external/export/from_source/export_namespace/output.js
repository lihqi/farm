//__farm_runtime.6463a55a.mjs:
 import __farmNodeModule from 'node:module';globalThis.nodeRequire = __farmNodeModule.createRequire(import.meta.url);(globalThis || window || global)['__farm_default_namespace__'] = {__FARM_TARGET_ENV__: 'node'};(globalThis || window || global)["__farm_default_namespace__"].__farm_module_system__.setPlugins([]);


//bundle2.js:
 (function(_){for(var r in _){_[r].__farm_resource_pot__='bundle2.js';(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.register(r,_[r])}})({"9488de80":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    function _export(target, all) {
        for(var name in all)Object.defineProperty(target, name, {
            enumerable: true,
            get: all[name]
        });
    }
    _export(exports, {
        bundle2A: function() {
            return bundle2A;
        },
        bundle2B: function() {
            return bundle2B;
        }
    });
    const bundle2A = "bundle2A";
    const bundle2B = "bundle2B";
}
,
"d1a94858":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    var _export_star = farmRequire("@swc/helpers/_/_export_star");
    _export_star._(farmRequire("9488de80"), exports);
}
,});

//index.js:
 import "./__farm_runtime.6463a55a.mjs";import "./bundle2.js";import * as __farm_external_module_node_fs from "node:fs";(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.setExternalModules({"node:fs": {...__farm_external_module_node_fs,__esModule:true}});(function(_){for(var r in _){_[r].__farm_resource_pot__='index_ae5c.js';(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.register(r,_[r])}})({"b31fbbb1":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    Object.defineProperty(exports, "fs", {
        enumerable: true,
        get: function() {
            return _nodefs;
        }
    });
    var _interop_require_wildcard = farmRequire("@swc/helpers/_/_interop_require_wildcard");
    var _nodefs = _interop_require_wildcard._(farmRequire("node:fs"));
    console.log("export namespace");
}
,
"b5d64806":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    function _export(target, all) {
        for(var name in all)Object.defineProperty(target, name, {
            enumerable: true,
            get: all[name]
        });
    }
    _export(exports, {
        bundle2: function() {
            return _bundle2index;
        },
        fs: function() {
            return _exportNamespace;
        }
    });
    var _interop_require_wildcard = farmRequire("@swc/helpers/_/_interop_require_wildcard");
    var _exportNamespace = _interop_require_wildcard._(farmRequire("b31fbbb1"));
    var _bundle2index = _interop_require_wildcard._(farmRequire("d1a94858"));
}
,});(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.setInitialLoadedResources(['bundle2.js']);(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.setDynamicModuleResourcesMap({  });var farmModuleSystem = (globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__;farmModuleSystem.bootstrap();var entry = farmModuleSystem.require("b5d64806");var fs=entry.fs;export { fs };var bundle2=entry.bundle2;export { bundle2 };