import fs from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const contractPath = process.argv[2];
if (!contractPath) {
  throw new Error("usage: node run-provider-smoke.mjs <contract.json>");
}

const contract = JSON.parse(fs.readFileSync(contractPath, "utf8"));
const outputDirectory = dirname(contractPath);
const createProvider = (
  await import(pathToFileURL(join(outputDirectory, contract.provider_file)).href)
).default;
const providerShim = await import(
  pathToFileURL(join(outputDirectory, contract.shim_file)).href
);

const memory = new WebAssembly.Memory({ initial: 2048, maximum: 4096 });
const provider = await createProvider({
  wasmMemory: memory,
  locateFile: (path) => join(outputDirectory, path),
  print: (text) => console.log(`[${contract.provider_module}] ${text}`),
  printErr: (text) => console.warn(`[${contract.provider_module}] ${text}`),
});

if (provider.wasmMemory && provider.wasmMemory !== memory) {
  throw new Error("provider did not use the shared WebAssembly.Memory");
}
const readProviderRevision =
  provider._boxddd_provider_abi_revision || provider.boxddd_provider_abi_revision;
if (typeof readProviderRevision !== "function") {
  throw new Error("provider is missing its ABI revision sentinel");
}
const providerRevision = readProviderRevision();
if (providerRevision !== contract.provider_bridge_revision) {
  throw new Error(
    `provider ABI revision ${providerRevision} does not match expected revision ${contract.provider_bridge_revision}`,
  );
}
providerShim.setBox3dProvider(provider);

const importObject = {
  env: { memory },
  [contract.provider_module]: {},
};
for (const name of contract.provider_imports) {
  const exported = providerShim[name];
  if (typeof exported !== "function") {
    throw new Error(`provider shim is missing export for ${name}`);
  }
  importObject[contract.provider_module][name] = exported;
}

function replaceBoxdddConsumer(consumer, exports) {
  const release = consumer.release;
  consumer.release = undefined;
  release();
  consumer.release = providerShim.acquireBoxdddConsumer(exports);
}

function assertSecondConsumerIsRejected(exports) {
  let rejected = false;
  try {
    providerShim.acquireBoxdddConsumer(exports);
  } catch (error) {
    if (!String(error).includes("already has an active Rust consumer")) {
      throw error;
    }
    rejected = true;
  }
  if (!rejected) {
    throw new Error("the provider accepted a simultaneous Rust consumer");
  }
}

const appBytes = fs.readFileSync(join(outputDirectory, contract.app_wasm_file));
const appModule = new WebAssembly.Module(appBytes);
const metricExports = {
  dropMillimeters: { name: "boxddd_provider_drop_millimeters" },
  rayHitMillimeters: { name: "boxddd_provider_ray_hit_millimeters" },
  shapeCastPermyriad: { name: "boxddd_provider_shape_cast_permyriad" },
  jointErrorMillimeters: { name: "boxddd_provider_joint_error_millimeters" },
  eventProvenanceMask: {
    name: "boxddd_provider_event_provenance_mask",
    exact: 2047,
  },
  foundationLifecycleMask: {
    name: "boxddd_provider_foundation_lifecycle_mask",
    exact: 4095,
  },
};

function requireAppExports(exports) {
  for (const name of contract.required_app_exports) {
    if (typeof exports[name] !== "function") {
      throw new Error(`${name} export is missing from Rust wasm`);
    }
  }
}

function runMetric(exports, spec) {
  const value = exports[spec.name]();
  if (value < 0) {
    throw new Error(`${spec.name} failed with code ${value}`);
  }
  if (spec.exact !== undefined && value !== spec.exact) {
    throw new Error(
      `${spec.name} returned ${value}; expected exact diagnostic ${spec.exact}`,
    );
  }
  return value;
}

function collectMetrics(exports) {
  const metrics = {};
  for (const [label, spec] of Object.entries(metricExports)) {
    metrics[label] = runMetric(exports, spec);
  }
  return metrics;
}

function exerciseTeardownPoisoning(instance, consumer) {
  const actualExports = instance.exports;
  const failingExports = Object.fromEntries(Object.entries(actualExports));
  failingExports.boxddd_debug_shape_destroy = () => {
    throw new Error("intentional provider teardown failure");
  };
  replaceBoxdddConsumer(consumer, failingExports);

  let failedTeardownShapeCount;
  try {
    failedTeardownShapeCount =
      actualExports.boxddd_provider_teardown_debug_shape_count();
  } finally {
    replaceBoxdddConsumer(consumer, actualExports);
  }
  if (failedTeardownShapeCount !== 1) {
    throw new Error(
      `teardown failure probe created ${failedTeardownShapeCount} debug shapes; expected 1`,
    );
  }

  const pendingErrors = provider.boxdddDebugDrawErrors?.size || 0;
  if (pendingErrors !== 0) {
    throw new Error(`provider retained ${pendingErrors} debug teardown errors`);
  }

  const poisonedTeardown =
    actualExports.boxddd_provider_teardown_debug_shape_count();
  if (poisonedTeardown !== -15) {
    throw new Error(
      `teardown callback failure did not poison the active Foundation: ${poisonedTeardown}`,
    );
  }
}

let baselineMetrics;
for (let cycle = 1; cycle <= 2; cycle += 1) {
  const instance = await WebAssembly.instantiate(appModule, importObject);
  requireAppExports(instance.exports);
  const consumer = {
    release: providerShim.acquireBoxdddConsumer(instance.exports),
  };

  try {
    assertSecondConsumerIsRejected(instance.exports);
    const code = instance.exports.boxddd_provider_smoke();
    if (code !== 0) {
      throw new Error(`boxddd provider smoke cycle ${cycle} failed with code ${code}`);
    }

    const metrics = collectMetrics(instance.exports);
    if (baselineMetrics === undefined) {
      baselineMetrics = metrics;
    } else if (JSON.stringify(metrics) !== JSON.stringify(baselineMetrics)) {
      throw new Error(
        `provider metrics changed after full teardown: ${JSON.stringify(baselineMetrics)} !== ${JSON.stringify(metrics)}`,
      );
    }

    exerciseTeardownPoisoning(instance, consumer);

    console.log(
      `boxddd provider smoke cycle ${cycle} passed: ` +
        `drop_mm=${metrics.dropMillimeters}, ` +
        `ray_hit_mm=${metrics.rayHitMillimeters}, ` +
        `shape_cast_permyriad=${metrics.shapeCastPermyriad}, ` +
        `joint_error_mm=${metrics.jointErrorMillimeters}, ` +
        `event_mask=${metrics.eventProvenanceMask}, ` +
        `foundation_mask=${metrics.foundationLifecycleMask}`,
    );
  } finally {
    if (consumer.release) {
      consumer.release();
    }
  }
}
