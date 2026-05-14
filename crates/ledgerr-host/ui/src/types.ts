export interface CytoscapeNodeData {
  id: string;
  label: string;
  kind: string;
  parent?: string;
  z_layer?: string;
  semantic_type?: string;
}

export interface CytoscapeNode {
  data: CytoscapeNodeData;
}

export interface CytoscapeEdgeData {
  id: string;
  source: string;
  target: string;
  label: string;
}

export interface CytoscapeEdge {
  data: CytoscapeEdgeData;
}

export interface CytoscapeGraph {
  nodes: CytoscapeNode[];
  edges: CytoscapeEdge[];
}
