// 3D pointer-anchored inspection card rendering process memory hexdumps and disassembly.
import * as THREE from "three";
import {
  bootGlyphEngine,
  GlyphAtlas,
  CodeGrid,
  bytesToHexView,
  decorateMemoryGrid,
} from "@glyph3d/core";
import { api, errorMessage } from "../shared/api.js";
import type { MemoryRead, MemoryMap } from "../shared/api-types.js";
import type { Region, RenderNode } from "./types.js";
import type { RegionHoverTarget } from "./contracts.js";
import { Display } from "../shared/display.js";
import { layoutMaps, findNearestRegion } from "./model.js";

let sharedAtlas: GlyphAtlas | null = null;
let initPromise: Promise<GlyphAtlas | null> | null = null;

export async function initGlyph3D(): Promise<GlyphAtlas | null> {
  if (sharedAtlas) return sharedAtlas;
  if (initPromise) return initPromise;
  initPromise = (async () => {
    try {
      sharedAtlas = await bootGlyphEngine({
        fontUrl: "/fonts/Cousine-Regular.ttf",
        wasmUrl: "/wasm/hb.wasm",
        fontSize: 48,
        atlasSize: 2048,
      });
      return sharedAtlas;
    } catch (e) {
      console.warn("[glyph3d] Engine boot skipped or unavailable:", e);
      return null;
    }
  })();
  return initPromise;
}

export type InspectionMode = "hex" | "disasm";

export interface DecodedInsn {
  address: bigint;
  bytesHex: string;
  mnemonic: string;
  operands: string;
}

const REGS64 = [
  "rax",
  "rcx",
  "rdx",
  "rbx",
  "rsp",
  "rbp",
  "rsi",
  "rdi",
  "r8",
  "r9",
  "r10",
  "r11",
  "r12",
  "r13",
  "r14",
  "r15",
];
const REGS32 = [
  "eax",
  "ecx",
  "edx",
  "ebx",
  "esp",
  "ebp",
  "esi",
  "edi",
  "r8d",
  "r9d",
  "r10d",
  "r11d",
  "r12d",
  "r13d",
  "r14d",
  "r15d",
];
const REGS16 = [
  "ax",
  "cx",
  "dx",
  "bx",
  "sp",
  "bp",
  "si",
  "di",
  "r8w",
  "r9w",
  "r10w",
  "r11w",
  "r12w",
  "r13w",
  "r14w",
  "r15w",
];
const REGS8_REX = [
  "al",
  "cl",
  "dl",
  "bl",
  "spl",
  "bpl",
  "sil",
  "dil",
  "r8b",
  "r9b",
  "r10b",
  "r11b",
  "r12b",
  "r13b",
  "r14b",
  "r15b",
];
const REGS8_NO_REX = ["al", "cl", "dl", "bl", "ah", "ch", "dh", "bh"];

const JCC_NAMES = [
  "jo",
  "jno",
  "jb",
  "jnb",
  "jz",
  "jnz",
  "jbe",
  "ja",
  "js",
  "jns",
  "jp",
  "jnp",
  "jl",
  "jge",
  "jle",
  "jg",
];

const GRP1_NAMES = ["add", "or", "adc", "sbb", "and", "sub", "xor", "cmp"];

/** Disassemble x86-64 instruction sequence from raw byte window. */
export function decodeX86_64(
  bytes: Uint8Array,
  baseAddr: bigint,
): DecodedInsn[] {
  const insns: DecodedInsn[] = [];
  let cursor = 0;
  const n = bytes.length;

  while (cursor < n) {
    const insnStart = cursor;
    const insnAddr = baseAddr + BigInt(insnStart);

    let hasRex = false;
    let rexW = false,
      rexR = false,
      rexX = false,
      rexB = false;
    let opSize16 = false;

    // Prefixes
    while (cursor < n) {
      const b = bytes[cursor];
      if (b === 0x66) {
        opSize16 = true;
        cursor++;
      } else if (
        b === 0x67 ||
        b === 0xf0 ||
        b === 0xf2 ||
        b === 0xf3 ||
        b === 0x2e ||
        b === 0x36 ||
        b === 0x3e ||
        b === 0x26 ||
        b === 0x64 ||
        b === 0x65
      ) {
        cursor++;
      } else if (b >= 0x40 && b <= 0x4f) {
        hasRex = true;
        rexW = (b & 0x08) !== 0;
        rexR = (b & 0x04) !== 0;
        rexX = (b & 0x02) !== 0;
        rexB = (b & 0x01) !== 0;
        cursor++;
      } else {
        break;
      }
    }

    if (cursor >= n) break;

    const opSize: 16 | 32 | 64 = rexW ? 64 : opSize16 ? 16 : 32;

    const regName = (idx: number, size: 8 | 16 | 32 | 64 = opSize) => {
      const i = idx & 15;
      if (size === 64) return REGS64[i];
      if (size === 32) return REGS32[i];
      if (size === 16) return REGS16[i];
      return hasRex ? REGS8_REX[i] : i < 8 ? REGS8_NO_REX[i] : REGS8_REX[i];
    };

    const readI8 = () => {
      if (cursor >= n) return 0;
      const val = bytes[cursor++];
      return val & 0x80 ? val - 0x100 : val;
    };

    const readI32 = () => {
      if (cursor + 4 > n) {
        cursor = n;
        return 0;
      }
      const val =
        bytes[cursor] |
        (bytes[cursor + 1] << 8) |
        (bytes[cursor + 2] << 16) |
        (bytes[cursor + 3] << 24);
      cursor += 4;
      return val;
    };

    const readU32 = () => readI32() >>> 0;

    const readU64Hex = () => {
      let hex = "";
      for (let j = 7; j >= 0; j--) {
        const b = cursor + j < n ? bytes[cursor + j] : 0;
        hex += b.toString(16).padStart(2, "0");
      }
      cursor += Math.min(8, n - cursor);
      return hex.replace(/^0+(?!$)/, "");
    };

    const decodeRm = () => {
      if (cursor >= n) return { rmStr: "??", regIdx: 0 };
      const modrm = bytes[cursor++];
      const mod = (modrm >> 6) & 3;
      const regIdx = ((modrm >> 3) & 7) | (rexR ? 8 : 0);
      const rmBase = (modrm & 7) | (rexB ? 8 : 0);

      if (mod === 3) {
        return { rmStr: regName(rmBase, opSize), regIdx };
      }

      const ptrSize =
        opSize === 64
          ? "qword ptr"
          : opSize === 32
            ? "dword ptr"
            : opSize === 16
              ? "word ptr"
              : "byte ptr";
      let baseStr = "";
      let indexStr = "";
      let dispStr = "";

      if ((modrm & 7) === 4) {
        // SIB
        if (cursor >= n) return { rmStr: `${ptrSize} [??]`, regIdx };
        const sib = bytes[cursor++];
        const scale = 1 << ((sib >> 6) & 3);
        const indexIdx = ((sib >> 3) & 7) | (rexX ? 8 : 0);
        const baseIdx = (sib & 7) | (rexB ? 8 : 0);
        if (indexIdx !== 4) {
          indexStr =
            scale === 1
              ? ` + ${REGS64[indexIdx]}`
              : ` + ${REGS64[indexIdx]}*${scale}`;
        }
        if (mod === 0 && (sib & 7) === 5) {
          const d = readI32();
          dispStr = `0x${(d >>> 0).toString(16)}`;
          return { rmStr: `${ptrSize} [${dispStr}${indexStr}]`, regIdx };
        }
        baseStr = REGS64[baseIdx];
      } else if (mod === 0 && (modrm & 7) === 5) {
        const disp = readI32();
        const nextAddr = insnAddr + BigInt(cursor - insnStart);
        const target = nextAddr + BigInt(disp);
        return {
          rmStr: `${ptrSize} [rip + 0x${(disp >>> 0).toString(16)}] <0x${target.toString(16)}>`,
          regIdx,
        };
      } else {
        baseStr = REGS64[rmBase];
      }

      if (mod === 1) {
        const d8 = readI8();
        if (d8 !== 0)
          dispStr =
            d8 > 0 ? ` + 0x${d8.toString(16)}` : ` - 0x${(-d8).toString(16)}`;
      } else if (mod === 2) {
        const d32 = readI32();
        if (d32 !== 0)
          dispStr =
            d32 > 0
              ? ` + 0x${(d32 >>> 0).toString(16)}`
              : ` - 0x${(-d32 >>> 0).toString(16)}`;
      }

      return { rmStr: `${ptrSize} [${baseStr}${indexStr}${dispStr}]`, regIdx };
    };

    const b = bytes[cursor++];
    let mnemonic = "";
    let operands = "";

    if (b === 0x90) {
      mnemonic = "nop";
    } else if (b === 0xcc) {
      mnemonic = "int3";
    } else if (b === 0xc3) {
      mnemonic = "ret";
    } else if (b === 0xc9) {
      mnemonic = "leave";
    } else if (b >= 0x50 && b <= 0x57) {
      mnemonic = "push";
      operands = regName((b & 7) | (rexB ? 8 : 0), 64);
    } else if (b >= 0x58 && b <= 0x5f) {
      mnemonic = "pop";
      operands = regName((b & 7) | (rexB ? 8 : 0), 64);
    } else if (b === 0x68) {
      mnemonic = "push";
      operands = `0x${readU32().toString(16)}`;
    } else if (b === 0x6a) {
      mnemonic = "push";
      operands = `0x${(readI8() & 0xff).toString(16)}`;
    } else if (b >= 0xb8 && b <= 0xbf) {
      mnemonic = "mov";
      const dst = regName((b & 7) | (rexB ? 8 : 0), rexW ? 64 : 32);
      operands = `${dst}, 0x${rexW ? readU64Hex() : readU32().toString(16)}`;
    } else if (b === 0x89) {
      mnemonic = "mov";
      const { rmStr, regIdx } = decodeRm();
      operands = `${rmStr}, ${regName(regIdx)}`;
    } else if (b === 0x8b) {
      mnemonic = "mov";
      const { rmStr, regIdx } = decodeRm();
      operands = `${regName(regIdx)}, ${rmStr}`;
    } else if (b === 0x8d) {
      mnemonic = "lea";
      const { rmStr, regIdx } = decodeRm();
      operands = `${regName(regIdx)}, ${rmStr}`;
    } else if (b === 0x85) {
      mnemonic = "test";
      const { rmStr, regIdx } = decodeRm();
      operands = `${rmStr}, ${regName(regIdx)}`;
    } else if (b === 0x39) {
      mnemonic = "cmp";
      const { rmStr, regIdx } = decodeRm();
      operands = `${rmStr}, ${regName(regIdx)}`;
    } else if (b === 0x3b) {
      mnemonic = "cmp";
      const { rmStr, regIdx } = decodeRm();
      operands = `${regName(regIdx)}, ${rmStr}`;
    } else if (b === 0x31) {
      mnemonic = "xor";
      const { rmStr, regIdx } = decodeRm();
      operands = `${rmStr}, ${regName(regIdx)}`;
    } else if (b === 0x33) {
      mnemonic = "xor";
      const { rmStr, regIdx } = decodeRm();
      operands = `${regName(regIdx)}, ${rmStr}`;
    } else if (b === 0x01) {
      mnemonic = "add";
      const { rmStr, regIdx } = decodeRm();
      operands = `${rmStr}, ${regName(regIdx)}`;
    } else if (b === 0x03) {
      mnemonic = "add";
      const { rmStr, regIdx } = decodeRm();
      operands = `${regName(regIdx)}, ${rmStr}`;
    } else if (b === 0x29) {
      mnemonic = "sub";
      const { rmStr, regIdx } = decodeRm();
      operands = `${rmStr}, ${regName(regIdx)}`;
    } else if (b === 0x2b) {
      mnemonic = "sub";
      const { rmStr, regIdx } = decodeRm();
      operands = `${regName(regIdx)}, ${rmStr}`;
    } else if (b === 0x83) {
      const { rmStr, regIdx } = decodeRm();
      mnemonic = GRP1_NAMES[regIdx & 7];
      operands = `${rmStr}, 0x${(readI8() & 0xff).toString(16)}`;
    } else if (b === 0x81) {
      const { rmStr, regIdx } = decodeRm();
      mnemonic = GRP1_NAMES[regIdx & 7];
      operands = `${rmStr}, 0x${readU32().toString(16)}`;
    } else if (b === 0xc7) {
      mnemonic = "mov";
      const { rmStr } = decodeRm();
      operands = `${rmStr}, 0x${readU32().toString(16)}`;
    } else if (b === 0xe8) {
      mnemonic = "call";
      const rel = readI32();
      const target = insnAddr + BigInt(5) + BigInt(rel);
      operands = `0x${target.toString(16)}`;
    } else if (b === 0xe9) {
      mnemonic = "jmp";
      const rel = readI32();
      const target = insnAddr + BigInt(5) + BigInt(rel);
      operands = `0x${target.toString(16)}`;
    } else if (b === 0xeb) {
      mnemonic = "jmp";
      const rel = readI8();
      const target = insnAddr + BigInt(2) + BigInt(rel);
      operands = `0x${target.toString(16)}`;
    } else if (b >= 0x70 && b <= 0x7f) {
      mnemonic = JCC_NAMES[b & 15];
      const rel = readI8();
      const target = insnAddr + BigInt(2) + BigInt(rel);
      operands = `0x${target.toString(16)}`;
    } else if (b === 0x0f) {
      if (cursor < n) {
        const b2 = bytes[cursor++];
        if (b2 === 0x05) {
          mnemonic = "syscall";
        } else if (b2 >= 0x80 && b2 <= 0x8f) {
          mnemonic = JCC_NAMES[b2 & 15];
          const rel = readI32();
          const target = insnAddr + BigInt(6) + BigInt(rel);
          operands = `0x${target.toString(16)}`;
        } else if (b2 === 0x1f) {
          mnemonic = "nop";
          decodeRm();
        } else if (b2 === 0xb6) {
          mnemonic = "movzx";
          const { rmStr, regIdx } = decodeRm();
          operands = `${regName(regIdx)}, ${rmStr}`;
        } else {
          mnemonic = `.byte 0x0f, 0x${b2.toString(16).padStart(2, "0")}`;
        }
      } else {
        mnemonic = ".byte 0x0f";
      }
    } else if (b === 0xff) {
      const { rmStr, regIdx } = decodeRm();
      const op = regIdx & 7;
      if (op === 0) {
        mnemonic = "inc";
        operands = rmStr;
      } else if (op === 1) {
        mnemonic = "dec";
        operands = rmStr;
      } else if (op === 2) {
        mnemonic = "call";
        operands = rmStr;
      } else if (op === 4) {
        mnemonic = "jmp";
        operands = rmStr;
      } else if (op === 6) {
        mnemonic = "push";
        operands = rmStr;
      } else {
        mnemonic = "grp5";
        operands = rmStr;
      }
    } else {
      mnemonic = `.byte 0x${b.toString(16).padStart(2, "0")}`;
    }

    const insnLen = cursor - insnStart;
    const hexParts: string[] = [];
    for (let k = insnStart; k < cursor; k++) {
      hexParts.push(bytes[k].toString(16).padStart(2, "0"));
    }

    insns.push({
      address: insnAddr,
      bytesHex: hexParts.join(" "),
      mnemonic,
      operands,
    });

    if (insnLen === 0) cursor++;
  }

  return insns;
}

export const CUBE_WIDTH = 4.2;
export const CUBE_HEIGHT = 3.4;
export const CUBE_DEPTH = 2.0;

export class ProcessInspectionCard extends THREE.Group {
  public readonly grid: CodeGrid;
  public readonly cubeGroup: THREE.Group;
  public readonly cubeFrame: THREE.LineSegments;
  public readonly cubeGlass: THREE.Mesh;
  public readonly hitMesh: THREE.Mesh;
  public readonly cubePagePlates: THREE.LineSegments;
  public readonly pageIndicator: THREE.Line;
  public readonly leaderLine: THREE.Line;

  private chassisMat: THREE.LineBasicMaterial;
  private leaderGeo: THREE.BufferGeometry;
  private leaderPos: Float32Array;

  public mode: InspectionMode = "hex";
  public locked = false;

  public pageIndex = 0;
  public totalPages = 1;
  public readonly pageSize = 512;
  public readonly SLICES_PER_PAGE = 3;
  public readonly ROWS_PER_SLICE = 7;
  public readonly BYTES_PER_SLICE = 128;
  public readonly MAX_READ_BYTES = 65536; // 64KB backend limit

  public currentBuffer: Uint8Array | null = null;
  public currentBufferAddr = 0n;
  public currentBufferLen = 0;
  public currentRegion: Region | null = null;
  public currentHoveredRegion: Region | null = null;
  public currentProcessNode: RenderNode | null = null;
  public currentBytes: Uint8Array | null = null;
  public currentAddress = 0n;

  private currentProcessKey = "";
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;
  private hideTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly HIDE_GRACE_MS = 160;
  private fetchAbortController: AbortController | null = null;
  private lastHitPoint: THREE.Vector3 | null = null;
  private memoryCache = new Map<string, { bytes: Uint8Array; time: number }>();
  private mapsFetchAttempted = new Set<string>();

  private getCache(
    processKey: string,
    address: bigint,
    length: number,
  ): Uint8Array | null {
    const key = `${processKey}:${address.toString(16)}:${length}`;
    const entry = this.memoryCache.get(key);
    if (!entry) return null;
    if (performance.now() - entry.time > 8000) {
      this.memoryCache.delete(key);
      return null;
    }
    return entry.bytes;
  }

  private putCache(
    processKey: string,
    address: bigint,
    length: number,
    bytes: Uint8Array,
  ) {
    const key = `${processKey}:${address.toString(16)}:${length}`;
    if (this.memoryCache.size >= 64) {
      const oldestKey = this.memoryCache.keys().next().value;
      if (oldestKey) this.memoryCache.delete(oldestKey);
    }
    this.memoryCache.set(key, { bytes, time: performance.now() });
  }

  private cancelDebounce() {
    if (this.debounceTimer !== null) {
      clearTimeout(this.debounceTimer);
      this.debounceTimer = null;
    }
  }

  private abortInFlight() {
    if (this.fetchAbortController) {
      this.fetchAbortController.abort();
      this.fetchAbortController = null;
    }
  }

  private cancelPending() {
    this.cancelDebounce();
    this.abortInFlight();
  }

  constructor(atlas: GlyphAtlas) {
    super();

    this.cubeGroup = new THREE.Group();
    this.add(this.cubeGroup);

    // 3D Chassis wireframe box
    const boxGeo = new THREE.BoxGeometry(CUBE_WIDTH, CUBE_HEIGHT, CUBE_DEPTH);
    const edgesGeo = new THREE.EdgesGeometry(boxGeo);
    this.chassisMat = new THREE.LineBasicMaterial({
      color: 0x4bd6bf,
      transparent: true,
      opacity: 0.85,
    });
    this.cubeFrame = new THREE.LineSegments(edgesGeo, this.chassisMat);
    this.cubeGroup.add(this.cubeFrame);

    // 3D Tinted translucent glass shell
    const glassMat = new THREE.MeshBasicMaterial({
      color: 0x050914,
      transparent: true,
      opacity: 0.82,
      depthWrite: false,
      side: THREE.DoubleSide,
    });
    this.cubeGlass = new THREE.Mesh(boxGeo, glassMat);
    this.cubeGroup.add(this.cubeGlass);

    // Generous invisible hit volume (1.35x) for reliable wheel scrolling and raycast picking
    const hitBoxGeo = new THREE.BoxGeometry(
      CUBE_WIDTH * 1.35,
      CUBE_HEIGHT * 1.35,
      CUBE_DEPTH * 1.5,
    );
    const hitMat = new THREE.MeshBasicMaterial({ visible: false });
    this.hitMesh = new THREE.Mesh(hitBoxGeo, hitMat);
    this.cubeGroup.add(this.hitMesh);

    // 3D internal page depth slices (stacked wireframe cards inside the cube)
    const pagePlatesGeo = new THREE.BufferGeometry();
    const platePositions: number[] = [];
    const halfW = CUBE_WIDTH * 0.46;
    const halfH = CUBE_HEIGHT * 0.44;
    for (const zSlice of [0.4, 0.0, -0.4]) {
      platePositions.push(
        -halfW,
        -halfH,
        zSlice,
        halfW,
        -halfH,
        zSlice,
        halfW,
        -halfH,
        zSlice,
        halfW,
        halfH,
        zSlice,
        halfW,
        halfH,
        zSlice,
        -halfW,
        halfH,
        zSlice,
        -halfW,
        halfH,
        zSlice,
        -halfW,
        -halfH,
        zSlice,
      );
    }
    pagePlatesGeo.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(platePositions, 3),
    );
    const plateMat = new THREE.LineBasicMaterial({
      color: 0x4bd6bf,
      transparent: true,
      opacity: 0.2,
    });
    this.cubePagePlates = new THREE.LineSegments(pagePlatesGeo, plateMat);
    this.cubeGroup.add(this.cubePagePlates);

    // Active page depth indicator (glowing accent bar)
    const indGeo = new THREE.BufferGeometry();
    const indPos = new Float32Array([
      -halfW,
      halfH + 0.04,
      0.0,
      halfW,
      halfH + 0.04,
      0.0,
    ]);
    indGeo.setAttribute("position", new THREE.BufferAttribute(indPos, 3));
    const indMat = new THREE.LineBasicMaterial({
      color: 0x50f0d0,
      transparent: true,
      opacity: 0.95,
    });
    this.pageIndicator = new THREE.Line(indGeo, indMat);
    this.cubeGroup.add(this.pageIndicator);

    // Mount CodeGrid onto front face of the cube
    this.grid = new CodeGrid(null, atlas, {
      name: "ProcessInspectionCard",
      showBackground: false,
      showFilename: true,
      padding: 10,
    });
    this.grid.scale.setScalar(0.062);
    this.grid.position.set(-1.8, 1.45, CUBE_DEPTH * 0.5 + 0.02);
    this.cubeGroup.add(this.grid);

    // Leader line from pillar to cube left-anchor
    this.leaderPos = new Float32Array(6);
    this.leaderGeo = new THREE.BufferGeometry();
    this.leaderGeo.setAttribute(
      "position",
      new THREE.BufferAttribute(this.leaderPos, 3),
    );
    const leaderMat = new THREE.LineBasicMaterial({
      color: 0x4bd6bf,
      transparent: true,
      opacity: 0.85,
      depthTest: true,
      depthWrite: false,
    });
    this.leaderLine = new THREE.Line(this.leaderGeo, leaderMat);
    this.leaderLine.frustumCulled = false;
    this.add(this.leaderLine);

    this.visible = false;
  }

  /** Update 3D cube position, embedded 3D isometric angle, and leader line. */
  public updateAnchor(
    hitPoint: THREE.Vector3,
    camera: THREE.Camera,
    _node?: RenderNode,
  ) {
    this.lastHitPoint = hitPoint.clone();

    // If locked in space, do not move the cube or leader line!
    if (this.locked) return;

    // Camera basis in world space
    const camRight = new THREE.Vector3(1, 0, 0).applyQuaternion(
      camera.quaternion,
    );
    const camUp = new THREE.Vector3(0, 1, 0).applyQuaternion(camera.quaternion);
    const camForward = new THREE.Vector3(0, 0, -1).applyQuaternion(
      camera.quaternion,
    );
    const toCam = camForward.clone().negate();

    // Position the cube offset from the hitPoint on the pillar:
    // Placed to the camera's right (+camRight * 3.4) and slightly toward camera (+toCam * 1.2)
    const cubePos = hitPoint
      .clone()
      .addScaledVector(camRight, 3.4)
      .addScaledVector(toCam, 1.2);

    this.cubeGroup.position.copy(cubePos);

    // Orientation:
    // Faces the user with a gentle ~18° yaw inward toward the screen center / pillar,
    // upright matching the camera's vertical axis.
    const yaw = -0.32; // ~18° inward toward screen center
    const localZ = new THREE.Vector3()
      .addScaledVector(toCam, Math.cos(yaw))
      .addScaledVector(camRight, Math.sin(yaw))
      .normalize();

    const localY = camUp.clone();
    const localX = new THREE.Vector3().crossVectors(localY, localZ).normalize();
    localY.crossVectors(localZ, localX).normalize();

    const rotMat = new THREE.Matrix4().makeBasis(localX, localY, localZ);
    this.cubeGroup.quaternion.setFromRotationMatrix(rotMat);
    this.cubeGroup.updateMatrixWorld();

    // Leader line from hitPoint on the pillar to cube's left anchor
    const leftAnchor = this.cubeGroup.localToWorld(
      new THREE.Vector3(-CUBE_WIDTH * 0.5, 0, 0),
    );

    this.leaderPos[0] = hitPoint.x;
    this.leaderPos[1] = hitPoint.y;
    this.leaderPos[2] = hitPoint.z;

    this.leaderPos[3] = leftAnchor.x;
    this.leaderPos[4] = leftAnchor.y;
    this.leaderPos[5] = leftAnchor.z;

    this.leaderGeo.attributes.position.needsUpdate = true;
  }

  /** Keep cube anchored as camera navigates (ignored if locked). */
  public updateCamera(camera: THREE.Camera) {
    if (
      !this.visible ||
      this.locked ||
      !this.lastHitPoint ||
      !this.currentProcessNode
    )
      return;
    this.updateAnchor(this.lastHitPoint, camera, this.currentProcessNode);
  }

  public isLocked(): boolean {
    return this.locked;
  }

  public isInteractive(): boolean {
    return this.visible;
  }

  public hitTest(ray: THREE.Raycaster): boolean {
    if (!this.visible) return false;
    this.hitMesh.updateMatrixWorld();
    return ray.intersectObject(this.hitMesh).length > 0;
  }

  public keepAlive() {
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
  }

  public hasTarget(id: string): boolean {
    return (
      this.visible &&
      (this.currentProcessKey === id ||
        this.currentProcessNode?.identity.pid.toString() === id)
    );
  }

  public lock() {
    if (!this.visible || !this.currentProcessNode) return;
    this.locked = true;
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
    this.updateLockedVisuals(true);
    this.renderActivePage();
  }

  public unlock() {
    this.locked = false;
    this.updateLockedVisuals(false);
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
    this.cancelPending();
    this.visible = false;
  }

  public toggleLock() {
    if (this.locked) {
      this.unlock();
    } else {
      this.lock();
    }
  }

  private updateLockedVisuals(locked: boolean) {
    const color = locked ? 0xffb84d : 0x4bd6bf;
    this.chassisMat.color.setHex(color);
    (this.leaderLine.material as THREE.LineBasicMaterial).color.setHex(color);
  }

  public scrollPages(delta: number) {
    if (!this.visible) return;
    const newIndex = Math.max(
      0,
      Math.min(this.totalPages - 1, this.pageIndex + delta),
    );
    if (newIndex === this.pageIndex) return;

    this.pageIndex = newIndex;
    this.updatePageIndicator();
    if (this.currentProcessNode) {
      this.renderActivePage();
    }
  }

  private updatePageIndicator() {
    if (!this.pageIndicator) return;
    const depthRange = CUBE_DEPTH * 0.8;
    const frac =
      this.totalPages > 1 ? this.pageIndex / (this.totalPages - 1) : 0;
    const indZ = CUBE_DEPTH * 0.4 - frac * depthRange;
    this.pageIndicator.position.z = indZ;
  }

  /** Toggle between raw hex memory view and disassembly view. */
  public async toggleMode(camera?: THREE.Camera) {
    this.mode = this.mode === "hex" ? "disasm" : "hex";
    if (this.currentProcessNode) {
      await this.renderActivePage();
    }
    if (camera && this.lastHitPoint && !this.locked) {
      this.updateAnchor(
        this.lastHitPoint,
        camera,
        this.currentProcessNode ?? undefined,
      );
    }
  }

  /** Process region hover event from selection raycast. */
  public handleHover(target: RegionHoverTarget | null, camera: THREE.Camera) {
    if (this.locked) return; // When locked in space, ignore hover transitions!

    if (!target) {
      if (this.visible && this.hideTimer === null) {
        this.hideTimer = setTimeout(() => {
          this.hideTimer = null;
          this.cancelDebounce();
          this.abortInFlight();
          this.hide();
        }, this.HIDE_GRACE_MS);
      }
      return;
    }

    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }

    const { process, region, hitPoint } = target;
    const processKey = `${process.identity.pid}:${process.identity.start_time_ticks}`;
    const sameProcess = this.currentProcessKey === processKey;

    if (!region) {
      if (
        process.regions.length === 0 &&
        !this.mapsFetchAttempted.has(processKey)
      ) {
        this.fetchProcessMapsAndHover(process, hitPoint, camera);
        return;
      }
      this.showKernelTaskNotice(process, hitPoint, camera);
      return;
    }

    this.updateAnchor(hitPoint, camera, process);
    this.visible = true;

    // Calculate total pages for all regions of this process
    const totalRegions = process.regions.length;
    this.totalPages = Math.max(
      1,
      Math.ceil(totalRegions / this.SLICES_PER_PAGE),
    );

    // Find the page index containing the hovered region
    const hoveredIndex = process.regions.findIndex(
      (r) => r.start === region.start && r.end === region.end,
    );
    const targetPage =
      hoveredIndex >= 0 ? Math.floor(hoveredIndex / this.SLICES_PER_PAGE) : 0;

    const samePage = sameProcess && this.pageIndex === targetPage;
    const sameHovered =
      sameProcess &&
      this.currentHoveredRegion?.start === region.start &&
      this.currentHoveredRegion?.end === region.end;

    this.currentProcessKey = processKey;
    this.currentProcessNode = process;
    this.currentRegion = region;
    this.currentHoveredRegion = region;
    this.pageIndex = targetPage;
    this.updatePageIndicator();

    // Default mode: if user hovered directly on executable code, default to disasm; otherwise hex
    if (!sameProcess) {
      if (region.executable && this.mode === "hex") {
        this.mode = "disasm";
      } else if (
        !region.executable &&
        this.mode === "disasm" &&
        region.pathname !== "[stack]"
      ) {
        this.mode = "hex";
      }
    }

    this.cancelDebounce();
    if (samePage && sameHovered) {
      return;
    }

    this.debounceTimer = setTimeout(() => {
      this.debounceTimer = null;
      this.renderActivePage();
    }, 50);
  }

  private showKernelTaskNotice(
    process: RenderNode,
    hitPoint: THREE.Vector3,
    camera: THREE.Camera,
  ) {
    const processKey = `${process.identity.pid}:${process.identity.start_time_ticks}`;
    this.cancelDebounce();
    this.abortInFlight();
    this.currentBuffer = null;
    this.currentBytes = null;
    this.currentAddress = 0n;
    this.currentProcessKey = processKey;
    this.currentProcessNode = process;
    this.currentRegion = null;
    this.currentHoveredRegion = null;
    this.totalPages = 1;
    this.pageIndex = 0;
    this.updateAnchor(hitPoint, camera, process);
    this.showNotice(
      "Kernel Task",
      `${process.name} [PID ${process.identity.pid}]`,
      [
        "This is a kernel thread / kernel task without user-space memory mappings.",
        "Kernel execution occurs entirely within supervisor address space.",
      ],
    );
  }

  private async fetchProcessMapsAndHover(
    process: RenderNode,
    hitPoint: THREE.Vector3,
    camera: THREE.Camera,
  ) {
    const processKey = `${process.identity.pid}:${process.identity.start_time_ticks}`;
    this.cancelDebounce();
    this.abortInFlight();
    this.currentBuffer = null;
    this.currentBytes = null;
    this.currentAddress = 0n;
    this.currentProcessKey = processKey;
    this.currentProcessNode = process;
    this.currentRegion = null;
    this.currentHoveredRegion = null;
    this.totalPages = 1;
    this.pageIndex = 0;
    this.updateAnchor(hitPoint, camera, process);
    await this.showNotice(
      "Loading Process Maps",
      `${process.name} [PID ${process.identity.pid}]`,
      ["Querying memory mappings from /proc..."],
    );

    try {
      const url = `/api/processes/maps?pid=${process.identity.pid}&start_time_ticks=${process.identity.start_time_ticks}`;
      const res = await api<{ maps?: MemoryMap[]; error?: string | null }>(url);
      if (this.currentProcessKey !== processKey || !this.visible) return;

      if (res.maps && res.maps.length > 0) {
        process.maps = res.maps;
        process.regions = layoutMaps(res.maps);
        const r = findNearestRegion(process.regions, hitPoint.z);
        if (r) {
          this.handleHover({ process, region: r, hitPoint }, camera);
          return;
        }
      }
      this.mapsFetchAttempted.add(processKey);
      if (res.error) {
        await this.showNotice(
          "Memory Maps Restricted",
          `${process.name} [PID ${process.identity.pid}]`,
          [
            res.error,
            "Cannot inspect memory without access to /proc/[pid]/maps.",
          ],
        );
        return;
      }
    } catch (e) {
      if (this.currentProcessKey !== processKey || !this.visible) return;
      console.warn(
        "Failed to fetch on-demand maps for PID",
        process.identity.pid,
        e,
      );
      this.mapsFetchAttempted.add(processKey);
    }

    if (this.currentProcessKey !== processKey || !this.visible) return;
    this.showKernelTaskNotice(process, hitPoint, camera);
  }

  public async renderActivePage() {
    const process = this.currentProcessNode;
    if (!process || !process.regions) return;

    const processKey = this.currentProcessKey;
    const totalRegions = process.regions.length;
    if (totalRegions === 0) return;

    this.totalPages = Math.max(
      1,
      Math.ceil(totalRegions / this.SLICES_PER_PAGE),
    );
    this.pageIndex = Math.max(0, Math.min(this.totalPages - 1, this.pageIndex));
    this.updatePageIndicator();

    const startIndex = this.pageIndex * this.SLICES_PER_PAGE;
    const endIndex = Math.min(totalRegions, startIndex + this.SLICES_PER_PAGE);
    const pageRegions = process.regions.slice(startIndex, endIndex);

    this.visible = true;

    const controller = new AbortController();
    this.fetchAbortController = controller;

    try {
      const slicePromises = pageRegions.map(async (r, i) => {
        const globalIndex = startIndex + i;
        const isHovered =
          this.currentHoveredRegion?.start === r.start &&
          this.currentHoveredRegion?.end === r.end;
        const startAddr = BigInt(r.start);
        const endAddr = BigInt(r.end);
        const addrRange = endAddr > startAddr ? endAddr - startAddr : 0n;

        if (!r.readable) {
          return {
            region: r,
            globalIndex,
            isHovered,
            bytes: null,
            address: startAddr,
            guard: true,
            error: null,
          };
        }

        let chunkStart = startAddr;
        if (isHovered && this.lastHitPoint && r.h > 0) {
          const spanZ = Math.max(0.001, r.h);
          const frac = Math.max(
            0,
            Math.min(1, (this.lastHitPoint.z - r.z) / spanZ),
          );
          if (addrRange > 128n) {
            const maxOffset = addrRange - 128n;
            const hoverOffset =
              BigInt(Math.floor(frac * Number(addrRange))) & ~15n;
            chunkStart =
              startAddr + (hoverOffset > maxOffset ? maxOffset : hoverOffset);
          }
        }

        const chunkLen = Math.max(
          16,
          Math.min(this.BYTES_PER_SLICE, Number(endAddr - chunkStart)),
        );

        const cached = this.getCache(processKey, chunkStart, chunkLen);
        if (cached) {
          return {
            region: r,
            globalIndex,
            isHovered,
            bytes: cached,
            address: chunkStart,
            guard: false,
            error: null,
          };
        }

        try {
          const addrHex = `0x${chunkStart.toString(16)}`;
          const url = `/api/processes/memory?pid=${process.identity.pid}&start_time_ticks=${process.identity.start_time_ticks}&address=${encodeURIComponent(addrHex)}&length=${chunkLen}`;
          const result = await api<MemoryRead>(url, {
            signal: controller.signal,
          });
          const bytes = new Uint8Array(result.bytes);
          this.putCache(processKey, chunkStart, chunkLen, bytes);
          return {
            region: r,
            globalIndex,
            isHovered,
            bytes,
            address: chunkStart,
            guard: false,
            error: null,
          };
        } catch (err: unknown) {
          if (controller.signal.aborted) return null;
          const msg = errorMessage(err);
          let friendly = msg;
          if (msg.includes("Bad address") || msg.includes("os error 14")) {
            friendly = "Uncommitted memory: pages not resident in physical RAM";
          } else if (
            msg.includes("Operation not permitted") ||
            msg.includes("os error 1")
          ) {
            friendly = "Access restricted by Yama ptrace policy";
          }
          return {
            region: r,
            globalIndex,
            isHovered,
            bytes: null,
            address: chunkStart,
            guard: false,
            error: friendly,
          };
        }
      });

      const slicesData = await Promise.all(slicePromises);
      if (controller.signal.aborted) return;
      if (this.currentProcessKey !== processKey || !this.visible) return;

      const validSlices = slicesData.filter(
        (s): s is NonNullable<typeof s> => s !== null,
      );
      await this.displayMultiSlicePage(process, validSlices);
    } finally {
      if (this.fetchAbortController === controller) {
        this.fetchAbortController = null;
      }
    }
  }

  private appendHexRows(lines: string[], bytes: Uint8Array, baseAddr: bigint) {
    const maxRows = Math.min(this.ROWS_PER_SLICE, Math.ceil(bytes.length / 16));
    const addrNum = Number(baseAddr & 0xffffffffffffn);
    const dump = bytesToHexView(bytes.subarray(0, maxRows * 16), {
      cols: 16,
      baseOffset: addrNum,
      addrWidth: 12,
    });
    for (const row of dump.split("\n")) {
      if (row.trim()) lines.push(row);
    }
  }

  private appendDisasmRows(
    lines: string[],
    bytes: Uint8Array,
    baseAddr: bigint,
    isHovered: boolean,
    highlights: {
      line: number;
      startCol: number;
      endCol: number;
      color: { r: number; g: number; b: number };
    }[],
  ) {
    const insns = decodeX86_64(bytes, baseAddr);
    if (insns.length === 0) {
      this.appendHexRows(lines, bytes, baseAddr);
      return;
    }
    const maxInsns = Math.min(insns.length, this.ROWS_PER_SLICE);
    for (let k = 0; k < maxInsns; k++) {
      const insn = insns[k];
      const isTop = k === 0 && isHovered;
      const ripTag = (isTop ? "[=>RIP]" : `[+${k}]`).padEnd(8, " ");
      const addrStr = `0x${insn.address.toString(16).padStart(12, "0")}`;
      const bytesStr = insn.bytesHex.padEnd(16, " ");
      const mnem = insn.mnemonic.padEnd(8, " ");
      const lineIdx = lines.length;
      lines.push(
        `${ripTag}  ${addrStr}  ${bytesStr}  ${mnem}  ${insn.operands}`,
      );
      if (isTop) {
        highlights.push({
          line: lineIdx,
          startCol: 0,
          endCol: 70,
          color: { r: 1.0, g: 0.85, b: 0.2 },
        });
      }
    }
  }

  private appendStackRows(
    lines: string[],
    bytes: Uint8Array,
    baseAddr: bigint,
    isHovered: boolean,
    highlights: {
      line: number;
      startCol: number;
      endCol: number;
      color: { r: number; g: number; b: number };
    }[],
  ) {
    let count = 0;
    for (
      let off = 0;
      off + 8 <= bytes.length && count < this.ROWS_PER_SLICE;
      off += 8, count++
    ) {
      const wordAddr = baseAddr + BigInt(off);
      let wordVal = 0n;
      for (let j = 7; j >= 0; j--) {
        wordVal = (wordVal << 8n) | BigInt(bytes[off + j]);
      }
      const isTop = count === 0 && isHovered;
      const tag = isTop
        ? "[=>RSP]"
        : `[+0x${off.toString(16).padStart(3, "0")}]`;
      const lineIdx = lines.length;
      lines.push(
        `${tag.padEnd(10, " ")}  0x${wordAddr.toString(16).padStart(12, "0")}:  0x${wordVal.toString(16).padStart(16, "0")}`,
      );
      if (isTop) {
        highlights.push({
          line: lineIdx,
          startCol: 0,
          endCol: 60,
          color: { r: 0.4, g: 0.9, b: 1.0 },
        });
      }
    }
  }

  private async displayMultiSlicePage(
    node: RenderNode,
    slices: {
      region: Region;
      globalIndex: number;
      isHovered: boolean;
      bytes: Uint8Array | null;
      address: bigint;
      guard: boolean;
      error: string | null;
    }[],
  ) {
    if (this.grid._memoryEdges) this.grid._memoryEdges.clear();
    this.grid.clearAllHighlights();

    const lines: string[] = [];
    interface HighlightSpec {
      line: number;
      startCol: number;
      endCol: number;
      color: { r: number; g: number; b: number };
    }
    const highlights: HighlightSpec[] = [];
    const total = node.regions.length;

    for (let si = 0; si < slices.length; si++) {
      const slice = slices[si];
      const r = slice.region;
      const startAddr = BigInt(r.start);
      const endAddr = BigInt(r.end);
      const sizeBytes = Number(endAddr - startAddr);
      const sizeStr =
        sizeBytes >= 1048576
          ? `${(sizeBytes / 1048576).toFixed(1)}MB`
          : sizeBytes >= 1024
            ? `${(sizeBytes / 1024).toFixed(0)}KB`
            : `${sizeBytes}B`;

      const perms = Display.permissions(r);
      const rawName = r.pathname || "anonymous";
      const name = rawName.startsWith("/")
        ? rawName.split("/").pop() || rawName
        : rawName;
      const tag = slice.isHovered
        ? `►►► [HOVERED ${slice.globalIndex + 1}/${total}]`
        : `─── [MAPPING ${slice.globalIndex + 1}/${total}]`;
      const bannerPrefix = `${tag} 0x${slice.address.toString(16)} · ${perms} · ${name} (${sizeStr}) `;
      const banner = bannerPrefix.padEnd(76, "─");
      const bannerLineIdx = lines.length;
      lines.push(banner);

      if (slice.isHovered) {
        highlights.push({
          line: bannerLineIdx,
          startCol: 0,
          endCol: banner.length,
          color: { r: 1.0, g: 0.82, b: 0.2 },
        });
      } else if (r.executable) {
        highlights.push({
          line: bannerLineIdx,
          startCol: 0,
          endCol: banner.length,
          color: { r: 0.3, g: 0.85, b: 1.0 },
        });
      } else if (slice.guard) {
        highlights.push({
          line: bannerLineIdx,
          startCol: 0,
          endCol: banner.length,
          color: { r: 0.95, g: 0.45, b: 0.45 },
        });
      } else {
        highlights.push({
          line: bannerLineIdx,
          startCol: 0,
          endCol: banner.length,
          color: { r: 0.4, g: 0.85, b: 0.65 },
        });
      }

      if (slice.guard) {
        lines.push(
          "  [PROT_NONE guard / memory reservation: non-readable mapping]",
        );
        lines.push(`  Virtual range: ${r.start} → ${r.end}`);
        lines.push(
          `  Pathname:      ${r.pathname || "anonymous guard region"}`,
        );
      } else if (slice.error) {
        lines.push(
          `  [Read Fault at 0x${slice.address.toString(16)}: ${slice.error}]`,
        );
        lines.push(`  Mapping:       ${r.start} → ${r.end} (${perms})`);
      } else if (slice.bytes && slice.bytes.length > 0) {
        const bytes = slice.bytes;
        const sliceAddr = slice.address;

        if (slice.isHovered) {
          this.currentBuffer = bytes;
          this.currentBufferAddr = sliceAddr;
          this.currentBufferLen = bytes.length;
          this.currentBytes = bytes;
          this.currentAddress = sliceAddr;
        }

        if (this.mode === "disasm" && r.pathname === "[stack]") {
          this.appendStackRows(
            lines,
            bytes,
            sliceAddr,
            slice.isHovered,
            highlights,
          );
        } else if (this.mode === "disasm" && r.executable) {
          this.appendDisasmRows(
            lines,
            bytes,
            sliceAddr,
            slice.isHovered,
            highlights,
          );
        } else {
          this.appendHexRows(lines, bytes, sliceAddr);
        }
      } else {
        lines.push("  [Zero bytes returned or empty memory region]");
      }

      if (si < slices.length - 1) {
        lines.push("");
      }
    }

    const lockTag = this.locked
      ? "🔒 LOCKED [Scroll/Wheel/[ ] to page · Click/ESC to unlock]"
      : "[Click to Lock · Scroll to page]";
    const pageTag = `PAGE ${this.pageIndex + 1}/${this.totalPages} (${total} slices)`;
    const modeTag = this.mode === "disasm" ? "DISASM" : "HEX";
    const header = `${node.name} [PID ${node.identity.pid}] · ${pageTag} · ${modeTag} · ${lockTag}`;

    await this.grid.loadFile(header, lines.join("\n"));

    for (const h of highlights) {
      this.grid.highlightRange(h.line, h.startCol, h.line, h.endCol, h.color);
    }
  }

  /** Display a notice message when memory read is restricted or failed. */
  public async showNotice(title: string, message: string, details?: string[]) {
    this.visible = true;
    if (this.grid._memoryEdges) this.grid._memoryEdges.clear();
    this.grid.clearAllHighlights();

    const lines = [
      `-- ${title} --`,
      "",
      message,
      ...(details ? ["", ...details] : []),
    ];
    await this.grid.loadFile(`NOTICE: ${title}`, lines.join("\n"));
    this.grid.highlightRange(0, 0, 0, lines[0].length, {
      r: 1.0,
      g: 0.65,
      b: 0.2,
    });
  }

  public hide() {
    if (this.locked) return; // Locked in space!
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
    this.cancelPending();
    this.visible = false;
  }

  public dispose() {
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
    this.cancelPending();
    this.grid.dispose();
    this.cubeFrame.geometry.dispose();
    (this.cubeFrame.material as THREE.Material).dispose();
    this.cubeGlass.geometry.dispose();
    (this.cubeGlass.material as THREE.Material).dispose();
    this.hitMesh.geometry.dispose();
    (this.hitMesh.material as THREE.Material).dispose();
    this.cubePagePlates.geometry.dispose();
    (this.cubePagePlates.material as THREE.Material).dispose();
    this.pageIndicator.geometry.dispose();
    (this.pageIndicator.material as THREE.Material).dispose();
    this.leaderGeo.dispose();
    (this.leaderLine.material as THREE.Material).dispose();
  }
}
