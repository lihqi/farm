//index.js:
 import __farmNodeModule from 'node:module';globalThis.nodeRequire = __farmNodeModule.createRequire(import.meta.url);(globalThis || window || global)['__farm_default_namespace__'] = {__FARM_TARGET_ENV__: 'node'};(globalThis || window || global)["__farm_default_namespace__"].__farm_module_system__.setPlugins([]);
import * as __farm_external_module_node_fs from "node:fs";(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.setExternalModules({"node:fs": {...__farm_external_module_node_fs,__esModule:true}});(function(_){for(var r in _){_[r].__farm_resource_pot__='index_7eea.js';(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.register(r,_[r])}})({"632ff088":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    var _nodefs = farmRequire("node:fs");
    console.log("external 2", _nodefs.read);
}
,
"9d5a7b13":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    var _nodefs = farmRequire("node:fs");
    console.log("external 1", _nodefs.read);
}
,
"b5d64806":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    farmRequire("9d5a7b13");
    farmRequire("632ff088");
    farmRequire("dea409d9");
}
,
"dea409d9":function  (module, exports, farmRequire, farmDynamicRequire) {
    "use strict";
    Object.defineProperty(exports, "__esModule", {
        value: true
    });
    var _nodefs = farmRequire("node:fs");
    console.log("external 3", _nodefs.read);
}
,});(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.setInitialLoadedResources([]);(globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__.setDynamicModuleResourcesMap({  });var farmModuleSystem = (globalThis || window || global)['__farm_default_namespace__'].__farm_module_system__;farmModuleSystem.bootstrap();var entry = farmModuleSystem.require("b5d64806");