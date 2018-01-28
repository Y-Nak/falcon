//! A universal means of representing location in a Falcon program
//!
//! We have two basic types of locations in Falcon:
//!
//! `RefProgramLocation`, and its companion `RefFunctionLocation`. These are program locations, 
//! "Applied," to a program.
//!
//! `ProgramLocation`, and its companion, `FunctionLocation`. These are program locations
//! independent of a program.
//!
//! We will normally deal with `RefProgramLocation`. However, as  `RefProgramLocation` has a
//! lifetime dependent on a specific `Program`, it can sometimes to be difficult to use.
//! Therefor, we have `ProgramLocation`, which is an Owned type in its own right with no
//! references.


use il::*;
use std::fmt;

/// A location applied to a `Program`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RefProgramLocation<'p> {
    function: &'p Function,
    function_location: RefFunctionLocation<'p>
}


impl<'p> RefProgramLocation<'p> {
    /// Create a new `RefProgramLocation` in the given `Program`.
    pub fn new(
        function: &'p Function,
        function_location: RefFunctionLocation<'p>)
    -> RefProgramLocation<'p> {

        RefProgramLocation {
            function: function,
            function_location: function_location
        }
    }

    /// Create a new `RefProgramLocation` in the given `Program` by finding the
    /// first `Instruction` with the given address.
    pub fn from_address(program: &'p Program, address: u64)
    -> Option<RefProgramLocation<'p>> {

        for function in program.functions() {
            for block in function.blocks() {
                for instruction in block.instructions() {
                    if let Some(iaddress) = instruction.address() {
                        if iaddress == address {
                            return Some(RefProgramLocation::new(&function,
                                RefFunctionLocation::Instruction(block, instruction)));
                        }
                    }
                }
            }
        }

        None
    }

    /// Create a new `RefProgramLocation` in the given `Program` by finding the
    /// first `Instruction` in the given function.
    pub fn from_function(function: &Function) -> Option<RefProgramLocation> {
        function.control_flow_graph().entry().map(|entry|
            function.block(entry).map(|block|
                RefProgramLocation::new(
                    function,
                    block.instructions().first().map(|instruction|
                        RefFunctionLocation::Instruction(block, instruction)
                    ).unwrap_or(RefFunctionLocation::EmptyBlock(block))
                )
            )
        ).unwrap_or(None)
    }

    /// Get the function for this `RefProgramLocation`.
    pub fn function(&self) -> &Function {
        &self.function
    }

    /// Get the `RefFunctionLocation` for this `RefProgramLocation`
    pub fn function_location(&self) -> &RefFunctionLocation {
        &self.function_location
    }

    /// If this `RefProgramLocation` references a `Block`, get that `Block`.
    pub fn block(&self) -> Option<&Block> {
        self.function_location.block()
    }

    /// If this `RefProgramLocation` references an `Instruction`, get that
    /// `Instruction`.
    pub fn instruction(&self) -> Option<&Instruction> {
        self.function_location.instruction()
    }

    /// If this `RefProgramLocation` references an `Edge`, get that `Edge`
    pub fn edge(&self) -> Option<&Edge> {
        self.function_location.edge()
    }

    /// If this `RefProgramLocation` is referencing an `Instruction` which has
    /// an address set, return that address.
    pub fn address(&self) -> Option<u64> {
        if let Some(instruction) = self.function_location.instruction() {
            return instruction.address();
        }
        None
    }

    /// Apply this `RefProgramLocation` to another `Program`.
    ///
    /// This works by locating the location in the other `Program` based on
    /// `Function`, `Block`, and `Instruction` indices.
    pub fn migrate<'m>(&self, program: &'m Program) -> Result<RefProgramLocation<'m>> {
        let function = program.function(self.function().index().unwrap())
            .ok_or(ErrorKind::ProgramLocationMigration(
                format!("Could not find function {}", self.function.index().unwrap())))?;
        let function_location = match self.function_location {
            RefFunctionLocation::Instruction(block, instruction) => {
                let block = function.block(block.index())
                    .ok_or(ErrorKind::ProgramLocationMigration(
                        format!("Could not find block {}", block.index())))?;
                let instruction = block.instruction(instruction.index())
                    .ok_or(ErrorKind::ProgramLocationMigration(
                        format!("Could not find instruction {}", instruction.index())))?;
                RefFunctionLocation::Instruction(block, instruction)
            },
            RefFunctionLocation::Edge(edge) => {
                let edge = function.edge(edge.head(), edge.tail())
                    .ok_or(ErrorKind::ProgramLocationMigration(
                        format!("Could not find edge {},{}", edge.head(), edge.tail())))?;
                RefFunctionLocation::Edge(edge)
            },
            RefFunctionLocation::EmptyBlock(block) => {
                let block = function.block(block.index())
                    .ok_or(ErrorKind::ProgramLocationMigration(
                        format!("Could not find empty block {}", block.index())))?;
                RefFunctionLocation::EmptyBlock(block)
            }
        };
        Ok(RefProgramLocation {
            function: function,
            function_location: function_location
        })
    }


    fn instruction_backward(&self, block: &'p Block, instruction: &Instruction)
    -> Result<Vec<RefProgramLocation<'p>>> {
        let instructions = block.instructions();
        for i in (0..instructions.len()).rev() {
            if instructions[i].index() == instruction.index() {
                if i > 0 {
                    let instruction = &instructions[i - 1];
                    return Ok(vec![RefProgramLocation::new(self.function,
                        RefFunctionLocation::Instruction(block, instruction))]);
                }
                let edges = match self.function
                                      .control_flow_graph()
                                      .edges_in(block.index()) {
                    Some(edges) => edges,
                    None => bail!("Could not find block {} in function {:?}",
                        block.index(),
                        self.function.index())
                };
                let mut locations = Vec::new();
                for edge in edges {
                    locations.push(RefProgramLocation::new(self.function,
                        RefFunctionLocation::Edge(edge)));
                }
                return Ok(locations);
            }
        }

        Err(format!("Could not find instruction {} in block {} in function {:?}",
            instruction.index(),
            block.index(),
            self.function.index()).into())
    }


    fn edge_backward(&self, edge: &'p Edge) -> Result<Vec<RefProgramLocation<'p>>> {
        let block = match self.function.block(edge.head()) {
            Some(block) => block,
            None => bail!("Could not find block {} in function {:?}",
                edge.head(), self.function.index())
        };

        let instructions = block.instructions();
        if instructions.is_empty() {
            Ok(vec![RefProgramLocation::new(self.function,
                RefFunctionLocation::EmptyBlock(block))])
        }
        else {
            Ok(vec![RefProgramLocation::new(self.function,
                RefFunctionLocation::Instruction(block, instructions.last().unwrap()))])
        }
    }


    fn empty_block_backward(&self, block: &'p Block)
    -> Result<Vec<RefProgramLocation<'p>>> {

        let edges = match self.function
                              .control_flow_graph()
                              .edges_in(block.index()) {
            Some(edges) => edges,
            None => bail!("Could not find block {} in function {:?}",
                block.index(), self.function.index())
        };

        let mut locations = Vec::new();
        for edge in edges {
            locations.push(RefProgramLocation::new(self.function,
                RefFunctionLocation::Edge(edge)));
        }
        Ok(locations)
    }


    fn instruction_forward(&self, block: &'p Block, instruction: &Instruction)
    -> Result<Vec<RefProgramLocation<'p>>> {

        let instructions = block.instructions();
        for i in 0..instructions.len() {
            // We found the instruction.
            if instructions[i].index() == instruction.index() {
                // Is there another instruction in this block?
                if i + 1 < instructions.len() {
                    // Return the next instruction
                    let instruction = &instructions[i + 1];
                    return Ok(vec![RefProgramLocation::new(self.function,
                        RefFunctionLocation::Instruction(block, instruction))]);
                }
                // No next instruction, return edges out of the block
                let edges = match self.function
                                      .control_flow_graph()
                                      .edges_out(block.index()) {
                    Some(edges) => edges,
                    None => bail!("Could not find block {} in function {:?}",
                        block.index(),
                        self.function.index())
                };
                let mut locations = Vec::new();
                for edge in edges {
                    locations.push(RefProgramLocation::new(self.function,
                        RefFunctionLocation::Edge(edge)));
                }
                return Ok(locations);
            }
        }

        Err(format!("Could not find instruction {} in block {} in function {:?}",
            instruction.index(),
            block.index(), 
            self.function.index()).into())
    }


    fn edge_forward(&self, edge: &'p Edge) -> Result<Vec<RefProgramLocation<'p>>> {
        let block = match self.function.block(edge.tail()) {
            Some(block) => block,
            None => bail!("Could not find block {} in function {:?}",
                edge.tail(), self.function.index())
        };

        let instructions = block.instructions();
        if instructions.is_empty() {
            Ok(vec![RefProgramLocation::new(self.function,
                RefFunctionLocation::EmptyBlock(block))])
        }
        else {
            Ok(vec![RefProgramLocation::new(self.function,
                RefFunctionLocation::Instruction(block, &instructions[0]))])
        }
    }


    fn empty_block_forward(&self, block: &'p Block)
    -> Result<Vec<RefProgramLocation<'p>>> {

        let edges = match self.function
                               .control_flow_graph()
                               .edges_out(block.index()) {
            Some(edges) => edges,
            None => bail!("Could not find block {} in function {:?}",
                block.index(), self.function.index())
        };

        let mut locations = Vec::new();
        for edge in edges {
            locations.push(RefProgramLocation::new(self.function,
                RefFunctionLocation::Edge(edge)));
        }

        Ok(locations)
    }


    /// Advance the `RefProgramLocation` forward.
    ///
    /// This does _not_ follow targets of `Operation::Brc`.
    pub fn forward(&self) -> Result<Vec<RefProgramLocation<'p>>> {
        match self.function_location {
            RefFunctionLocation::Instruction(block, instruction) => 
                self.instruction_forward(block, instruction),
            RefFunctionLocation::Edge(edge) => self.edge_forward(edge),
            RefFunctionLocation::EmptyBlock(block) =>self.empty_block_forward(block)
        }
    }


    /// Advance the `RefProgramLocation` backward.
    ///
    /// This does _not_ follow targets of `Operation::Brc`.
    pub fn backward(&self) -> Result<Vec<RefProgramLocation<'p>>> {
        match self.function_location {
            RefFunctionLocation::Instruction(block, instruction) =>
                self.instruction_backward(block, instruction),
            RefFunctionLocation::Edge(edge) => self.edge_backward(edge),
            RefFunctionLocation::EmptyBlock(block) => self.empty_block_backward(block)
        }
    }
}


impl<'f> fmt::Display for RefProgramLocation<'f> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.function.index() {
            Some(index) => write!(f, "0x{:x}:{}", index, self.function_location),
            None => write!(f, "{}", self.function_location)
        }
    }
}


/// A location applied to a `Function`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RefFunctionLocation<'f> {
    Instruction(&'f Block, &'f Instruction),
    Edge(&'f Edge),
    EmptyBlock(&'f Block)
}


impl<'f> RefFunctionLocation<'f> {
    /// If this `RefFunctionLocation` references a `Block`, get that `Block`.
    pub fn block(&self) -> Option<&Block> {
        match *self {
            RefFunctionLocation::Instruction(ref block, _) => Some(block),
            RefFunctionLocation::EmptyBlock(ref block) => Some(block),
            _ => None
        }
    }

    /// If this `RefFunctionLocation` references an `Instruction`, get that
    /// `Instruction`.
    pub fn instruction(&self) -> Option<&Instruction> {
        match *self {
            RefFunctionLocation::Instruction(_, ref instruction) => Some(instruction),
            _ => None
        }
    }

    /// If this `RefFunctionLocation` references an `Edge`, get that `Edge`.
    pub fn edge(&self) -> Option<&Edge> {
        match *self {
            RefFunctionLocation::Edge(ref edge) => Some(edge),
            _ => None
        }
    }
}


impl<'f> fmt::Display for RefFunctionLocation<'f> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            RefFunctionLocation::Instruction(ref block, ref instruction) => 
                write!(f, "0x{:x}:{}", block.index(), instruction),
            RefFunctionLocation::Edge(ref edge) => edge.fmt(f),
            RefFunctionLocation::EmptyBlock(ref empty_block) => empty_block.fmt(f)
        }
    }
}




/// A location independent of any specific instance of `Program`.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ProgramLocation {
    function_index: Option<u64>,
    function_location: FunctionLocation
}


impl ProgramLocation {
    /// Create a new `ProgramLocation` from a function index and `FunctionLocation`
    pub fn new(
        function_index: Option<u64>,
        function_location: FunctionLocation
    ) -> ProgramLocation {

        ProgramLocation {
            function_index: function_index,
            function_location: function_location
        }
    }


    /// "Apply" this `ProgramLocation` to a `Program`, returning a
    /// `RefProgramLocation`.
    pub fn apply<'p>(&self, program: &'p Program) -> Option<RefProgramLocation<'p>> {
        if self.function_index.is_none() {
            return None;
        }
        let function_index = self.function_index.unwrap();

        let function = match program.function(function_index) {
            Some(function) => function,
            None => { return None; }
        };
        let function_location = match self.function_location.apply(function) {
            Some(function_location) => function_location,
            None => { return None; }
        };
        Some(RefProgramLocation::new(function, function_location))
    }

    /// Get the `FunctionLocation` for this `ProgramLocation`
    pub fn function_location(&self) -> &FunctionLocation {
        &self.function_location
    }

    /// If this `ProgramLocation` has a valid `Block` target, return the index
    /// of that `Block`.
    pub fn block_index(&self) -> Option<u64> {
        self.function_location.block_index()
    }


    /// If this `ProgramLocation` has a valid `Instruction` target, return the
    /// index of that `Instruction`
    pub fn instruction_index(&self) -> Option<u64> {
        self.function_location.instruction_index()
    }
}


impl<'p> From<RefProgramLocation<'p>> for ProgramLocation {
    fn from(program_location: RefProgramLocation) -> ProgramLocation {
        ProgramLocation {
            function_index: program_location.function().index(),
            function_location: program_location.function_location.into()
        }
    }
}


impl fmt::Display for ProgramLocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.function_index {
            Some(function_index) => write!(f, "0x{:x}:{}", function_index, self.function_location),
            None => write!(f, "{}", self.function_location)
        }
    }
}


/// A location indepdent of any specific instance of `Function`.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FunctionLocation {
    Instruction(u64, u64),
    Edge(u64, u64),
    EmptyBlock(u64)
}


impl FunctionLocation {
    /// "Apply" this `FunctionLocation` to a `Function`, returning a
    /// `RefFunctionLocation`.
    pub fn apply<'f>(&self, function: &'f Function)
    -> Option<RefFunctionLocation<'f>> {

        match *self {
            FunctionLocation::Instruction(block_index, instruction_index) => {
                let block = match function.block(block_index) {
                    Some(block) => block,
                    None => { return None; }
                };
                let instruction = match block.instruction(instruction_index) {
                    Some(instruction) => instruction,
                    None => { return None; }
                };
                Some(RefFunctionLocation::Instruction(block, instruction))
            },
            FunctionLocation::Edge(head, tail) => {
                match function.edge(head, tail) {
                    Some(edge) => Some(RefFunctionLocation::Edge(edge)),
                    None => None
                }
            },
            FunctionLocation::EmptyBlock(block_index) => {
                match function.block(block_index) {
                    Some(block) => Some(RefFunctionLocation::EmptyBlock(block)),
                    None => None
                }
            }
        }
    }


    /// If this `FunctionLocation` has a valid `Block` target, return the index
    /// of that `Instruction`.
    pub fn block_index(&self) -> Option<u64> {
        match *self {
            FunctionLocation::Instruction(block_index, _) => Some(block_index),
            FunctionLocation::EmptyBlock(block_index) => Some(block_index),
            _ => None
        }
    }


    /// If this `FunctionLocation` has a valid `Instruction` target, return the
    /// index of that `Instruction`.
    pub fn instruction_index(&self) -> Option<u64> {
        match *self {
            FunctionLocation::Instruction(_, instruction_index) => Some(instruction_index),
            _ => None
        }
    }
}


impl<'f> From<RefFunctionLocation<'f>> for FunctionLocation {
    fn from(function_location: RefFunctionLocation) -> FunctionLocation {
        match function_location {
            RefFunctionLocation::Instruction(block, instruction) =>
                FunctionLocation::Instruction(block.index(), instruction.index()),
            RefFunctionLocation::Edge(edge) =>
                FunctionLocation::Edge(edge.head(), edge.tail()),
            RefFunctionLocation::EmptyBlock(block) =>
                FunctionLocation::EmptyBlock(block.index())
        }
    }
}


impl fmt::Display for FunctionLocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FunctionLocation::Instruction(block_index, instruction_index) =>
                write!(f, "0x{:X}:{:02X}", block_index, instruction_index),
            FunctionLocation::Edge(head_index, tail_index) =>
                write!(f, "(0x{:X}->0x{:X})", head_index, tail_index),
            FunctionLocation::EmptyBlock(block_index) =>
                write!(f, "0x{:X}", block_index)
        }
    }
}