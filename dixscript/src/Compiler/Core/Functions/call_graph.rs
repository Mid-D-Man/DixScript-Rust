use super::call_site::CallSite;
use std::collections::{HashMap, HashSet};
use std::fmt;
use crate::Compiler::AST::Position;

/// Directed graph of function calls in QuickFunctions
/// Each node is a function, each edge represents "function A calls function B"
///
/// Algorithm: DFS-based cycle detection with path tracking
/// Time Complexity: O(V + E) where V = functions, E = call edges
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// Adjacency list: function name → set of functions it calls
    adjacency_list: HashMap<String, HashSet<String>>,
    
    /// Call site details: caller → list of all call sites
    call_sites: HashMap<String, Vec<CallSite>>,
    
    /// All registered functions (including those that don't call anything)
    all_functions: HashSet<String>,
}

impl CallGraph {
    pub fn new() -> Self {
        CallGraph {
            adjacency_list: HashMap::new(),
            call_sites: HashMap::new(),
            all_functions: HashSet::new(),
        }
    }
    
    /// Register a function in the graph (even if it makes no calls)
    pub fn add_function(&mut self, function_name: String) {
        self.all_functions.insert(function_name.clone());
        self.adjacency_list.entry(function_name).or_default();
    }
    
    /// Add an edge: function 'caller' calls function 'callee'
    pub fn add_edge(&mut self, caller: String, callee: String, position: Position) {
        // Ensure both functions are registered
        self.add_function(caller.clone());
        self.add_function(callee.clone());
        
        // Add edge to adjacency list
        self.adjacency_list
            .get_mut(&caller)
            .unwrap()
            .insert(callee.clone());
        
        // Track call site for error reporting
        self.call_sites
            .entry(caller.clone())
            .or_default()
            .push(CallSite::new(caller, callee, position));
    }
    
    /// Quick check: does the graph contain any cycles?
    pub fn has_cycles(&self) -> bool {
        !self.detect_cycles().is_empty()
    }
    
    /// Detect all cycles using DFS
    /// Returns a list of cycles, where each cycle is a list of function names forming a loop
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();
        let mut current_path = Vec::new();
        
        // Try DFS from each unvisited node
        for node in self.adjacency_list.keys() {
            if !visited.contains(node) {
                self.dfs(
                    node,
                    &mut visited,
                    &mut recursion_stack,
                    &mut current_path,
                    &mut cycles,
                );
            }
        }
        
        cycles
    }
    
    /// DFS helper for cycle detection
    /// Returns true if a cycle was detected in this branch
    fn dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
        current_path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) -> bool {
        // Mark node as visited and add to recursion stack
        visited.insert(node.to_string());
        recursion_stack.insert(node.to_string());
        current_path.push(node.to_string());
        
        // Explore all neighbors
        if let Some(neighbors) = self.adjacency_list.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    // Unvisited neighbor - continue DFS
                    if self.dfs(neighbor, visited, recursion_stack, current_path, cycles) {
                        return true;
                    }
                } else if recursion_stack.contains(neighbor) {
                    // CYCLE DETECTED!
                    // Find where the cycle starts in our current path
                    if let Some(cycle_start_index) = current_path.iter().position(|n| *n == *neighbor) {
                        // Extract the cycle
                        let mut cycle = current_path[cycle_start_index..].to_vec();
                        
                        // Add the closing edge (back to start)
                        cycle.push(neighbor.clone());
                        
                        // Store the cycle
                        cycles.push(cycle);
                    }
                    
                    return true;
                }
                // else: visited but not in recursion stack = already explored branch, ignore
            }
        }
        
        // Backtrack: remove from path and recursion stack
        current_path.pop();
        recursion_stack.remove(node);
        
        false
    }
    
    /// Get all functions that the specified function calls (direct dependencies)
    pub fn get_callees(&self, function_name: &str) -> Vec<&str> {
        self.adjacency_list
            .get(function_name)
            .map(|set| set.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
    
    /// Get all functions that call the specified function (reverse dependencies)
    pub fn get_callers(&self, function_name: &str) -> Vec<&str> {
        self.adjacency_list
            .iter()
            .filter(|(_, callees)| callees.contains(function_name))
            .map(|(caller, _)| caller.as_str())
            .collect()
    }
    
    /// Check if a specific function is called by anyone
    pub fn is_function_called(&self, function_name: &str) -> bool {
        self.adjacency_list
            .values()
            .any(|callees| callees.contains(function_name))
    }
    
    /// Get detailed call site information for a specific caller → callee edge
    pub fn get_call_sites(&self, caller: &str, callee: &str) -> Vec<&CallSite> {
        self.call_sites
            .get(caller)
            .map(|sites| {
                sites
                    .iter()
                    .filter(|cs| cs.callee == callee)
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get all call sites for a specific caller
    pub fn get_all_call_sites(&self, caller: &str) -> Vec<&CallSite> {
        self.call_sites
            .get(caller)
            .map(|sites| sites.iter().collect())
            .unwrap_or_default()
    }
    
    /// Get topological sort of the call graph (if acyclic)
    /// Returns None if the graph contains cycles
    pub fn get_topological_sort(&self) -> Option<Vec<String>> {
        if self.has_cycles() {
            return None;
        }
        
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut result = Vec::new();
        let mut queue = Vec::new();
        
        // Calculate in-degree for each node
        for func in &self.all_functions {
            in_degree.insert(func.clone(), 0);
        }
        
        for callees in self.adjacency_list.values() {
            for callee in callees {
                *in_degree.get_mut(callee).unwrap() += 1;
            }
        }
        
        // Enqueue nodes with in-degree 0
        for (func, &degree) in &in_degree {
            if degree == 0 {
                queue.push(func.clone());
            }
        }
        
        // Kahn's algorithm
        while let Some(node) = queue.pop() {
            result.push(node.clone());
            
            if let Some(neighbors) = self.adjacency_list.get(&node) {
                for neighbor in neighbors {
                    let degree = in_degree.get_mut(neighbor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push(neighbor.clone());
                    }
                }
            }
        }
        
        // If we didn't process all nodes, there's a cycle
        if result.len() == self.all_functions.len() {
            Some(result)
        } else {
            None
        }
    }
    
    /// Get statistics about the call graph
    pub fn get_statistics(&self) -> CallGraphStats {
        let total_edges = self.adjacency_list.values().map(|v| v.len()).sum();
        let max_out_degree = self.adjacency_list.values().map(|v| v.len()).max().unwrap_or(0);
        let functions_with_no_calls = self.adjacency_list.values().filter(|v| v.is_empty()).count();
        let functions_never_called = self.all_functions
            .iter()
            .filter(|f| !self.is_function_called(f))
            .count();
        let has_cycles = self.has_cycles();
        let cycle_count = self.detect_cycles().len();
        
        CallGraphStats {
            total_functions: self.all_functions.len(),
            total_edges,
            max_out_degree,
            functions_with_no_calls,
            functions_never_called,
            has_cycles,
            cycle_count,
        }
    }
    
    /// Generate a human-readable visualization (for debugging)
    pub fn to_debug_string(&self) -> String {
        use std::fmt::Write;
        
        let mut output = String::new();
        writeln!(output, "Call Graph:").unwrap();
        writeln!(output, "  Functions: {}", self.all_functions.len()).unwrap();
        writeln!(
            output,
            "  Edges: {}",
            self.adjacency_list.values().map(|v| v.len()).sum::<usize>()
        ).unwrap();
        writeln!(output).unwrap();
        
        let mut functions: Vec<_> = self.all_functions.iter().collect();
        functions.sort();
        
        for func in functions {
            let callees = self.get_callees(func);
            
            if !callees.is_empty() {
                writeln!(output, "  {} calls:", func).unwrap();
                let mut sorted_callees = callees.clone();
                sorted_callees.sort();
                
                for callee in sorted_callees {
                    let sites = self.get_call_sites(func, callee);
                    if let Some(site) = sites.first() {
                        if site.position.is_valid() {
                            writeln!(output, "    → {} at {}", callee, site.position).unwrap();
                        } else {
                            writeln!(output, "    → {}", callee).unwrap();
                        }
                    } else {
                        writeln!(output, "    → {}", callee).unwrap();
                    }
                }
            } else {
                writeln!(output, "  {} (no calls)", func).unwrap();
            }
        }
        
        output
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CallGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_debug_string())
    }
}

/// Statistics about a call graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallGraphStats {
    pub total_functions: usize,
    pub total_edges: usize,
    pub max_out_degree: usize,
    pub functions_with_no_calls: usize,
    pub functions_never_called: usize,
    pub has_cycles: bool,
    pub cycle_count: usize,
}

impl fmt::Display for CallGraphStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Functions: {}, Edges: {}, Max Out-Degree: {}, Leaf Functions: {}, Root Functions: {}, Cycles: {}",
            self.total_functions,
            self.total_edges,
            self.max_out_degree,
            self.functions_with_no_calls,
            self.functions_never_called,
            if self.has_cycles {
                format!("{} detected", self.cycle_count)
            } else {
                "None".to_string()
            }
        )
    }
}
