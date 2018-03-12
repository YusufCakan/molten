
use std::ptr;
use std::fmt;
use std::ffi::CString;

extern crate llvm_sys as llvm;
use self::llvm::prelude::*;
use self::llvm::core::*;

use export;
use abi::ABI;
use types::Type;
use utils::UniqueID;
use config::Options;
use session::Session;
use ast::{ AST, Pos };
use scope::{ Scope, ScopeRef, ScopeMapRef };

use llvm::lib_llvm::{ BuiltinDef, BuiltinMap, initialize_builtins };


pub type Value = Box<Compilable>;

impl Clone for Box<Compilable> {
    fn clone(&self) -> Box<Compilable> {
        self.box_clone()
    }
}

impl fmt::Debug for Box<Compilable> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.get_ref())
    }
}



pub fn from_abi(abi: &ABI, value: LLVMValueRef) -> Box<Compilable> {
    match *abi {
        ABI::Unknown | ABI::Molten => Box::new(Function(value)),
        ABI::C => Box::new(CFunction(value)),
        _ => panic!("Unsupported ABI for LLVM Compiling: {:?}", abi)
    }
}

pub fn from_type(ttype: &Type, value: LLVMValueRef) -> Box<Compilable> {
    match *ttype {
        Type::Function(_, _, ref abi) => from_abi(abi, value),
        _ => Box::new(Data(value))
    }
}


pub trait Compilable {
    fn box_clone(&self) -> Box<Compilable>;
    fn get_ref(&self) -> LLVMValueRef;

    //unsafe fn resolve(&self, data: &LLVM) -> Box<Compilable>;

    unsafe fn invoke(&self, data: &LLVM, unwind: Unwind, mut largs: Vec<LLVMValueRef>) -> LLVMValueRef {
        //LLVMBuildCall(data.builder, self.0, largs.as_mut_ptr(), largs.len() as u32, label("tmp"))
        match unwind {
            None => LLVMBuildCall(data.builder, self.get_ref(), largs.as_mut_ptr(), largs.len() as u32, label("thingies")),
            Some((then, catch)) => {
                LLVMBuildInvoke(data.builder, self.get_ref(), largs.as_mut_ptr(), largs.len() as u32, then, catch, label("tmp"))
                //LLVMSetUnwindDest(invoke, unwind.unwrap());
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data(pub LLVMValueRef);

impl Compilable for Data {
    fn box_clone(&self) -> Box<Compilable> {
        Box::new((*self).clone())
    }

    fn get_ref(&self) -> LLVMValueRef {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function(pub LLVMValueRef);

impl Compilable for Function {
    fn box_clone(&self) -> Box<Compilable> {
        Box::new((*self).clone())
    }

    fn get_ref(&self) -> LLVMValueRef {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CFunction(pub LLVMValueRef);

impl Compilable for CFunction {
    fn box_clone(&self) -> Box<Compilable> {
        Box::new((*self).clone())
    }

    fn get_ref(&self) -> LLVMValueRef {
        self.0
    }

    unsafe fn invoke(&self, data: &LLVM, unwind: Unwind, mut largs: Vec<LLVMValueRef>) -> LLVMValueRef {
        LLVMBuildCall(data.builder, self.0, largs.as_mut_ptr(), largs.len() as u32, label("tmp"))
    }
}


#[derive(Clone, Debug)]
pub struct Builtin(pub BuiltinFunction, pub Type);

impl Compilable for Builtin {
    fn box_clone(&self) -> Box<Compilable> {
        Box::new((*self).clone())
    }

    fn get_ref(&self) -> LLVMValueRef {
        ptr::null_mut()
    }

    unsafe fn invoke(&self, data: &LLVM, unwind: Unwind, largs: Vec<LLVMValueRef>) -> LLVMValueRef {
        (self.0).0(data, largs)
    }
}

#[derive(Clone)]
pub struct BuiltinFunction(pub unsafe fn(&LLVM, Vec<LLVMValueRef>) -> LLVMValueRef);

impl fmt::Debug for BuiltinFunction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "??")
    }
}




#[derive(Clone, Debug, PartialEq)]
pub struct Var(pub LLVMValueRef);

impl Compilable for Var {
    fn box_clone(&self) -> Box<Compilable> {
        Box::new((*self).clone())
    }

    fn get_ref(&self) -> LLVMValueRef {
        self.0
    }

}

#[derive(Clone, Debug, PartialEq)]
pub struct Global(pub LLVMValueRef);

impl Compilable for Global {
    fn box_clone(&self) -> Box<Compilable> {
        Box::new((*self).clone())
    }

    fn get_ref(&self) -> LLVMValueRef {
        self.0
    }
}



#[derive(Clone, Debug, PartialEq)]
pub struct TypeValue {
    pub structdef: Vec<(String, Type)>,
    pub value: LLVMTypeRef,
    pub vtable: Vec<(String, Type)>,
    pub vttype: Option<LLVMTypeRef>,
}


pub struct LLVM<'a> {
    pub session: &'a Session<Value, TypeValue>,
    pub map: ScopeMapRef<Value, TypeValue>,
    //pub builtins: TypeFunctionMap,
    pub builtins: BuiltinMap<'a>,
    pub functions: Vec<&'a AST>,
    pub classes: Vec<&'a AST>,
    pub context: LLVMContextRef,
    pub module: LLVMModuleRef,
    pub builder: LLVMBuilderRef,
    //pub funcpass: LLVMPassManagerRef,
}

type Unwind = Option<(LLVMBasicBlockRef, LLVMBasicBlockRef)>;


pub fn compile(builtins: &Vec<BuiltinDef>, session: &Session<Value, TypeValue>, module_name: &str, code: &Vec<AST>) {
    unsafe {
        compile_module(builtins, session, session.map.get_global(), module_name, code)
    }
}

unsafe fn compile_module(builtins: &Vec<BuiltinDef>, session: &Session<Value, TypeValue>, scope: ScopeRef<Value, TypeValue>, module_name: &str, code: &Vec<AST>) {
    let context = LLVMContextCreate();
    let module = LLVMModuleCreateWithName(label(module_name));
    let builder = LLVMCreateBuilderInContext(context);
    let data = &mut LLVM {
        session: session,
        map: session.map.clone(),
        builtins: BuiltinMap::new(),
        functions: Vec::new(),
        classes: Vec::new(),
        context: context,
        module: module,
        builder: builder
    };
    //LLVMSetDataLayout(data.module, label("e-m:e-i64:64-f80:128-n8:16:32:64-S128"));

    initialize_builtins(data, scope.clone(), builtins);
    collect_functions_vec(data, scope.clone(), code);
    declare_globals(data, scope.clone());
    for func in &data.functions {
        build_function_body(data, func);
    }

    let module_init_name = format!("init.{}", module_name.replace("/", "."));


    let ftype = Type::Function(vec!(), Box::new(Type::Object(String::from("Bool"), vec!())), ABI::Molten);
    let lftype = LLVMFunctionType(bool_type(data), ptr::null_mut(), 0, 0);
    let function = LLVMAddFunction(module, label(module_init_name.as_str()), lftype);
    //LLVMSetPersonalityFn(function, LLVMGetNamedFunction(data.module, label("__gxx_personality_v0")));
    LLVMPositionBuilderAtEnd(builder, LLVMAppendBasicBlockInContext(context, function, label("entry")));
    compile_vec(data, function, None, scope.clone(), code);
    LLVMBuildRet(builder, LLVMConstInt(bool_type(data), 1, 0));
    scope.borrow_mut().define(module_init_name.clone(), Some(ftype));
    scope.borrow_mut().assign(&module_init_name, Box::new(Function(function)));


    if !Options::as_ref().is_library {
        let pos = Pos::empty();
        let function_type = LLVMFunctionType(int_type(data), ptr::null_mut(), 0, 0);
        let function = LLVMAddFunction(module, b"main\0".as_ptr() as *const _, function_type);
        //LLVMSetPersonalityFn(function, LLVMGetNamedFunction(data.module, label("__gxx_personality_v0")));
        LLVMPositionBuilderAtEnd(builder, LLVMAppendBasicBlockInContext(context, function, label("entry")));
        compile_node(data, function, None, scope.clone(), &AST::Invoke(pos.clone(), Box::new(AST::Identifier(pos.clone(), String::from(module_init_name))), vec!(), Some(Type::Function(vec!(), Box::new(Type::Object(String::from("Bool"), vec!())), ABI::Molten))));
        LLVMBuildRet(builder, int_value(data, 0));
    }

    // Output to a file, and also a string for debugging
    //format!("{}.ll", module_name)
    LLVMPrintModuleToFile(module, label(session.target.as_str()), ptr::null_mut());

    if Options::as_ref().debug {
        println!("{}\n", CString::from_raw(LLVMPrintModuleToString(module)).into_string().unwrap());
    }

    let name = session.target.rsplitn(2, '.').collect::<Vec<&str>>()[1];
    export::write_exports(data.map.clone(), data.map.get_global(), format!("{}.dec", name).as_str(), code);

    LLVMDisposeBuilder(builder);
    LLVMDisposeModule(module);
    LLVMContextDispose(context);
}

unsafe fn declare_globals(data: &LLVM, scope: ScopeRef<Value, TypeValue>) {
    for node in &data.classes {
        if let AST::Class(_, (ref name, _), _, _, ref id) = **node {
            let tscope = data.map.get(id);
            let classdef = scope.borrow().get_class_def(name);
            let value = scope.borrow().get_type_value(name).unwrap();

            if value.vtable.len() > 0 {
                let mut methods = vec!();
                for (index, &(ref name, _)) in value.vtable.iter().enumerate() {
                    methods.push(classdef.borrow().get_variable_value(&name).unwrap().get_ref());
                }

                let vtype = value.vttype.unwrap();
                let name = format!("__{}_vtable", name);
                let global = LLVMAddGlobal(data.module, LLVMGetElementType(vtype), label(name.as_str()));
                LLVMSetInitializer(global, LLVMConstNamedStruct(vtype, methods.as_mut_ptr(), methods.len() as u32));
                LLVMSetLinkage(global, llvm::LLVMLinkage::LLVMLinkOnceAnyLinkage);
                scope.borrow_mut().assign(&name, Box::new(Global(global)));
            }
        }
    }

    let mut globals = vec!();
    for (name, sym) in &scope.borrow().names {
        if LLVMGetNamedFunction(data.module, label(name.as_str())).is_null() && !sym.ttype.as_ref().unwrap().is_overloaded() {
            let ltype = get_type(data, scope.clone(), sym.ttype.clone().unwrap(), true);
            let global = LLVMAddGlobal(data.module, ltype, label(name.as_str()));
            LLVMSetInitializer(global, null_value(ltype));
            globals.push((name.clone(), global));
        }
    }

    for (name, global) in globals {
        scope.borrow_mut().assign(&name, Box::new(Global(global)));
    }
}

unsafe fn compile_vec(data: &LLVM, func: LLVMValueRef, unwind: Unwind, scope: ScopeRef<Value, TypeValue>, code: &Vec<AST>) -> Value {
    let mut last: Value = Box::new(Data(zero_int(data)));
    for node in code {
        last = compile_node(data, func, unwind, scope.clone(), node);
    }
    last
}

unsafe fn compile_node(data: &LLVM, func: LLVMValueRef, unwind: Unwind, scope: ScopeRef<Value, TypeValue>, node: &AST) -> Value {
    debug!("COMPILE: {:?}", node);
    match *node {
        AST::Noop => Box::new(Data(ptr::null_mut())),
        AST::Nil(ref ttype) => Box::new(Data(null_value(get_type(data, scope.clone(), ttype.clone().unwrap(), true)))),
        AST::Boolean(ref num) => Box::new(Data(LLVMConstInt(bool_type(data), *num as u64, 0))),
        AST::Integer(ref num) => Box::new(Data(LLVMConstInt(int_type(data), *num as u64, 0))),
        AST::Real(ref num) => Box::new(Data(LLVMConstReal(real_type(data), *num))),
        AST::String(ref string) => Box::new(Data(LLVMBuildGlobalStringPtr(data.builder, label(string.as_str()), label("strc")))),

        AST::Block(_, ref body) => { compile_vec(data, func, unwind, scope, body) },

        AST::Invoke(ref pos, ref fexpr, ref args, ref stype) => {
            let (atypes, rtype, abi) = match stype.clone().unwrap() {
                Type::Function(atypes, rtype, abi) => (atypes, *rtype, abi),
                stype @ _ => panic!("TypeError: expected function type: {:?}", stype),
            };

            let mut largs = vec!();
            for (ttype, arg) in atypes.iter().zip(args.iter()) {
                let mut larg = compile_node(data, func, unwind, scope.clone(), arg).get_ref();
                let ltype = get_type(data, scope.clone(), ttype.clone(), true);
                if ltype != LLVMTypeOf(larg) {
                    // TODO this seems to cast to int as well as pointers, so maybe it's doing too much, at least without checking that it's supposed to
                    larg = LLVMBuildPointerCast(data.builder, larg, ltype, label("ptr"));
                }
                largs.push(larg);
            }

            /*
            let function = match **fexpr {
                AST::Accessor(ref pos, ref left, ref name, ref otype) => {
                    let tname = format!("{}", UniqueID::generate());
                    scope.borrow_mut().define(tname.clone(), otype.clone());
                    scope.borrow_mut().assign(&tname, from_type(otype.as_ref().unwrap(), largs[0]));
                    compile_node(data, func, unwind, scope.clone(), &AST::Accessor(pos.clone(), Box::new(AST::Identifier(pos.clone(), tname)), name.clone(), otype.clone()))
                    // TODO this is possibly very bad, but hopefully it works temporarily, for vtable
                    //compile_node(data, func, unwind, scope.clone(), &AST::Resolver(pos.clone(), Box::new(AST::Identifier(pos.clone(), stype.clone().unwrap().get_name().unwrap())), name.clone()))
                },
                _ => compile_node(data, func, unwind, scope.clone(), fexpr)
            };
            */

            let function = compile_node(data, func, unwind, scope.clone(), fexpr);
            if !function.get_ref().is_null() { LLVMDumpValue(function.get_ref()); }

            if !function.get_ref().is_null() {
                // Cast values to the function's declared type; this is a hack for typevar/generic arguments
                let mut lftype = LLVMTypeOf(function.get_ref());
                if LLVMGetTypeKind(lftype) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
                    lftype = LLVMGetElementType(lftype);
                }
                let mut ltypes = Vec::with_capacity(LLVMCountParamTypes(lftype) as usize);
                ltypes.set_len(LLVMCountParamTypes(lftype) as usize);
                LLVMGetParamTypes(lftype, ltypes.as_mut_ptr());
                for i in 0 .. ltypes.len() {
                    if ltypes[i] != LLVMTypeOf(largs[i]) {
                        debug!("{:?} -> {:?}", LLVMGetTypeKind(LLVMTypeOf(largs[i])), LLVMGetTypeKind(ltypes[i]));
                        largs[i] = build_generic_cast(data, largs[i], ltypes[i]);
                    }
                }
            }

            //LLVMBuildCall(data.builder, function, largs.as_mut_ptr(), largs.len() as u32, label("tmp").as_ptr())
            //let mut value = match unwind {
            //    None => LLVMBuildCall(data.builder, function.get_ref(), largs.as_mut_ptr(), largs.len() as u32, label("tmp")),
            //    Some((then, catch)) => {
            //        LLVMBuildInvoke(data.builder, function.get_ref(), largs.as_mut_ptr(), largs.len() as u32, then, catch, label("tmp"))
            //        //LLVMSetUnwindDest(invoke, unwind.unwrap());
            //    }
            //};
            let mut value = function.invoke(data, unwind, largs);

            if !function.get_ref().is_null() {
                let lrtype = get_type(data, scope.clone(), rtype.clone(), true);
                if lrtype != LLVMTypeOf(value) {
                    debug!("RETURN {:?} -> {:?}", LLVMGetTypeKind(LLVMTypeOf(value)), LLVMGetTypeKind(lrtype));
                    value = build_generic_cast(data, value, lrtype);
                }
            }
            from_type(&rtype, value)
        },

        AST::Function(_, ref name, _, _, _, ref id, ref abi) => {
            let fname = scope.borrow().get_full_name(name, id.clone());
            from_abi(abi, LLVMGetNamedFunction(data.module, label(fname.as_str())))
            //scope.borrow().get_variable_value(&name.as_ref().map_or_else(|| format!("anon{}", id), |name| name.clone())).unwrap()
        },

        AST::Identifier(ref pos, ref name) => {
            let pointer = match scope.borrow().get_variable_value(name) {
                Ok(x) => x,
                Err(_) => panic!("UnsetError:{:?}: use before assignment {:?}", pos, name),
            };
            let ttype = scope.borrow().get_variable_type(name).unwrap();
            if !pointer.get_ref().is_null() { debug!("IDENT: {:?} {:?}", LLVMGetValueKind(pointer.get_ref()), LLVMGetTypeKind(LLVMTypeOf(pointer.get_ref()))); }
            //if !pointer.get_ref().is_null() { LLVMDumpValue(pointer.get_ref()); }
            //if LLVMGetTypeKind(LLVMTypeOf(pointer)) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
            if !pointer.get_ref().is_null() {
                match LLVMGetValueKind(pointer.get_ref()) {
                    //llvm::LLVMValueKind::LLVMArgumentValueKind |
                    //llvm::LLVMValueKind::LLVMFunctionValueKind => pointer,
                    llvm::LLVMValueKind::LLVMInstructionValueKind |
                    llvm::LLVMValueKind::LLVMGlobalVariableValueKind => from_type(&ttype, LLVMBuildLoad(data.builder, pointer.get_ref(), label("tmp"))),
                    _ => pointer,
                }
            } else {
                pointer
            }

            //pointer
        },

        AST::Definition(_, (ref name, ref ttype), ref value) => {
            let ltype = get_type(data, scope.clone(), ttype.clone().unwrap(), true);
            let pointer: Value = if scope.borrow().is_global() {
                //LLVMAddGlobal(data.module, ltype, label(name.as_str()))
                //let global = LLVMGetNamedGlobal(data.module, label(name.as_str()));
                scope.borrow().get_variable_value(name).unwrap()
            } else {
                Box::new(Var(LLVMBuildAlloca(data.builder, ltype, label(name.as_str()))))
            };
            scope.borrow_mut().assign(name, pointer.clone());
            let value = compile_node(data, func, unwind, scope, value);
            LLVMBuildStore(data.builder, value.get_ref(), pointer.get_ref());
            value
        },

        // TODO you might need to create a new function to hold the body, so that it can be resumed from, and there also
        // might be an issue when you call a function one level down, and pass it the same unwind locations... it might effect all
        // functions, all the way down
        AST::Try(_, ref body, ref cases) => {
            /*
            // TODO need to add multiple cases, and also it doesn't work
            let try_block = LLVMAppendBasicBlockInContext(data.context, func, label("try"));
            let catch_block = LLVMAppendBasicBlockInContext(data.context, func, label("catch"));
            let finally_block = LLVMAppendBasicBlockInContext(data.context, func, label("finally"));

            LLVMBuildBr(data.builder, try_block);
            LLVMPositionBuilderAtEnd(data.builder, try_block);
            let body_value = compile_node(data, func, Some((finally_block, catch_block)), scope.clone(), body);
            LLVMBuildBr(data.builder, finally_block);

            LLVMPositionBuilderAtEnd(data.builder, catch_block);
            let pad = LLVMBuildLandingPad(data.builder, LLVMTypeOf(body_value), ptr::null_mut(), 0, label("pad"));
            LLVMSetCleanup(pad, 1);
            //LLVMAddClause(pad, );
            LLVMBuildBr(data.builder, finally_block);

            LLVMPositionBuilderAtEnd(data.builder, finally_block);
            //let phi = LLVMBuildPhi(data.builder, LLVMTypeOf(body_value), label("matchphi"));

            //LLVMAddIncoming(phi, values.as_mut_ptr(), do_blocks.as_mut_ptr(), values.len() as u32);
            //phi
            */
            Box::new(Data(null_value(str_type(data))))
        },

        AST::Raise(_, ref expr) => {
            //LLVMBuildResume(data.builder, compile_node(data, func, unwind, scope.clone(), expr))
            Box::new(Data(null_value(str_type(data))))
        },

        AST::SideEffect(_, ref op, ref args) => {
            // TODO This only handles two arguments, which is all that will be parsed, but you can do better... 
            let lexpr_block = LLVMAppendBasicBlockInContext(data.context, func, label(op));
            let rexpr_block = LLVMAppendBasicBlockInContext(data.context, func, label(op));
            let merge_block = LLVMAppendBasicBlockInContext(data.context, func, label(op));

            LLVMBuildBr(data.builder, lexpr_block);

            LLVMPositionBuilderAtEnd(data.builder, lexpr_block);
            let lexpr_value = compile_node(data, func, unwind, scope.clone(), &args[0]).get_ref();
            let test_type = match op.as_str() {
                "or" => llvm::LLVMIntPredicate::LLVMIntNE,
                "and" => llvm::LLVMIntPredicate::LLVMIntEQ,
                _ => panic!("NotImplementedError: attempted to compile invalid side effect operation: {}", op)
            };
            let is_enough = LLVMBuildICmp(data.builder, test_type, lexpr_value, null_value(LLVMTypeOf(lexpr_value)), label("is_enough"));
            LLVMBuildCondBr(data.builder, is_enough, merge_block, rexpr_block);

            LLVMPositionBuilderAtEnd(data.builder, rexpr_block);
            let rexpr_value = compile_node(data, func, unwind, scope.clone(), &args[1]).get_ref();
            LLVMBuildBr(data.builder, merge_block);

            LLVMPositionBuilderAtEnd(data.builder, merge_block);
            let phi = LLVMBuildPhi(data.builder, LLVMTypeOf(lexpr_value), label(op));

            let mut values = vec![lexpr_value, rexpr_value];
            let mut blocks = vec![lexpr_block, rexpr_block];

            LLVMAddIncoming(phi, values.as_mut_ptr(), blocks.as_mut_ptr(), 2);
            Box::new(Data(phi))
        },

        AST::If(_, ref cond, ref texpr, ref fexpr) => {
            let cond_value = compile_node(data, func, unwind, scope.clone(), cond).get_ref();
            let cond_zero = LLVMConstInt(LLVMTypeOf(cond_value), 0, 0);
            let is_nonzero = LLVMBuildICmp(data.builder, llvm::LLVMIntPredicate::LLVMIntNE, cond_value, cond_zero, label("is_nonzero"));

            let texpr_block = LLVMAppendBasicBlockInContext(data.context, func, label("ifthen"));
            let fexpr_block = LLVMAppendBasicBlockInContext(data.context, func, label("ifelse"));
            let merge_block = LLVMAppendBasicBlockInContext(data.context, func, label("ifend"));

            LLVMBuildCondBr(data.builder, is_nonzero, texpr_block, fexpr_block);


            LLVMPositionBuilderAtEnd(data.builder, texpr_block);
            let texpr_value = compile_node(data, func, unwind, scope.clone(), texpr).get_ref();
            LLVMBuildBr(data.builder, merge_block);
            let texpr_block = LLVMGetInsertBlock(data.builder);

            LLVMPositionBuilderAtEnd(data.builder, fexpr_block);
            let fexpr_value = compile_node(data, func, unwind, scope.clone(), fexpr).get_ref();
            LLVMBuildBr(data.builder, merge_block);
            let fexpr_block = LLVMGetInsertBlock(data.builder);

            LLVMPositionBuilderAtEnd(data.builder, merge_block);
            let phi = LLVMBuildPhi(data.builder, LLVMTypeOf(texpr_value), label("iftmp"));

            let mut values = vec![texpr_value, fexpr_value];
            let mut blocks = vec![texpr_block, fexpr_block];

            LLVMAddIncoming(phi, values.as_mut_ptr(), blocks.as_mut_ptr(), 2);
            Box::new(Data(phi))
        },

        AST::Match(_, ref cond, ref cases) => {
            let mut cond_blocks = vec!();
            let mut do_blocks = vec!();
            for _ in 0 .. cases.len() {
                cond_blocks.push(LLVMAppendBasicBlockInContext(data.context, func, label("matchcond")));
                do_blocks.push(LLVMAppendBasicBlockInContext(data.context, func, label("matchdo")));
            }
            let merge_block = LLVMAppendBasicBlockInContext(data.context, func, label("matchend"));
            cond_blocks.push(merge_block);

            let cond_value = compile_node(data, func, unwind, scope.clone(), cond).get_ref();
            LLVMBuildBr(data.builder, cond_blocks[0]);

            let mut values = vec!();
            for (i, &(ref case, ref expr)) in cases.iter().enumerate() {
                LLVMPositionBuilderAtEnd(data.builder, cond_blocks[i]);
                match *case {
                    AST::Underscore => { LLVMBuildBr(data.builder, do_blocks[i]); },
                    _ => {
                        let case_value = compile_node(data, func, unwind, scope.clone(), case).get_ref();
                        let is_true = LLVMBuildICmp(data.builder, llvm::LLVMIntPredicate::LLVMIntEQ, cond_value, case_value, label("is_true"));
                        LLVMBuildCondBr(data.builder, is_true, do_blocks[i], cond_blocks[i + 1]);
                    }
                }

                LLVMPositionBuilderAtEnd(data.builder, do_blocks[i]);
                values.push(compile_node(data, func, unwind, scope.clone(), expr).get_ref());
                LLVMBuildBr(data.builder, merge_block);
            }

            LLVMPositionBuilderAtEnd(data.builder, merge_block);
            let phi = LLVMBuildPhi(data.builder, LLVMTypeOf(values[0]), label("matchphi"));

            LLVMAddIncoming(phi, values.as_mut_ptr(), do_blocks.as_mut_ptr(), values.len() as u32);
            Box::new(Data(phi))
        },

        AST::While(_, ref cond, ref body) => {
            let before_block = LLVMAppendBasicBlockInContext(data.context, func, label("while"));
            let body_block = LLVMAppendBasicBlockInContext(data.context, func, label("whilebody"));
            let after_block = LLVMAppendBasicBlockInContext(data.context, func, label("whileend"));

            LLVMBuildBr(data.builder, before_block);
            LLVMPositionBuilderAtEnd(data.builder, before_block);
            let cond_value = compile_node(data, func, unwind, scope.clone(), cond).get_ref();
            let cond_zero = LLVMConstInt(LLVMTypeOf(cond_value), 0, 0);
            let is_nonzero = LLVMBuildICmp(data.builder, llvm::LLVMIntPredicate::LLVMIntNE, cond_value, cond_zero, label("is_nonzero"));
            LLVMBuildCondBr(data.builder, is_nonzero, body_block, after_block);

            LLVMPositionBuilderAtEnd(data.builder, body_block);
            //let body_value = compile_node(data, func, unwind, scope.clone(), body).get_ref();
            compile_node(data, func, unwind, scope.clone(), body).get_ref();
            LLVMBuildBr(data.builder, before_block);

            LLVMPositionBuilderAtEnd(data.builder, after_block);
            //let phi = LLVMBuildPhi(data.builder, LLVMTypeOf(body_value), label("whileend"));

            //let mut values = vec![body_value];
            //let mut blocks = vec![before_block];

            //LLVMAddIncoming(phi, values.as_mut_ptr(), blocks.as_mut_ptr(), 1);
            //phi
            //body_value
            Box::new(Data(null_value(str_type(data))))
        },

        AST::For(_, ref name, ref list, ref body, ref id) => {
            let lscope = data.map.get(id);

            let list_value = compile_node(data, func, unwind, scope.clone(), list).get_ref();
            let inc = LLVMBuildAlloca(data.builder, int_type(data), label("inc"));
            LLVMBuildStore(data.builder, zero_int(data), inc);

            let itype = get_type(data, lscope.clone(), lscope.borrow().get_variable_type(name).unwrap(), true);
            let item = LLVMBuildAlloca(data.builder, itype, label("item"));
            lscope.borrow_mut().assign(name, Box::new(Var(item)));

            let before_block = LLVMAppendBasicBlockInContext(data.context, func, label("for"));
            let body_block = LLVMAppendBasicBlockInContext(data.context, func, label("forbody"));
            let after_block = LLVMAppendBasicBlockInContext(data.context, func, label("forend"));

            LLVMBuildBr(data.builder, before_block);
            LLVMPositionBuilderAtEnd(data.builder, before_block);
            //let cond_value = compile_node(data, func, unwind, scope.clone(), cond).get_ref();

            let cond_value = LLVMBuildLoad(data.builder, inc, label("tmp"));
            let cond_length = build_call(data, "List_len", &mut vec!(list_value));
            let is_end = LLVMBuildICmp(data.builder, llvm::LLVMIntPredicate::LLVMIntSGE, cond_value, cond_length, label("is_end"));
            LLVMBuildCondBr(data.builder, is_end, after_block, body_block);

            LLVMPositionBuilderAtEnd(data.builder, body_block);
            let nextitem = build_cast_from_vartype(data, build_call(data, "List_get", &mut vec!(list_value, cond_value)), itype);
            LLVMBuildStore(data.builder, nextitem, item);

            let body_value = compile_node(data, func, unwind, lscope.clone(), body).get_ref();

            let next = LLVMBuildLoad(data.builder, inc, label("tmp"));
            let next = LLVMBuildAdd(data.builder, next, int_value(data, 1), label("tmp"));
            LLVMBuildStore(data.builder, next, inc);
            LLVMBuildBr(data.builder, before_block);

            LLVMPositionBuilderAtEnd(data.builder, after_block);
            //let phi = LLVMBuildPhi(data.builder, LLVMInt64TypeInContext(data.context), label("forend"));

            //let mut values = vec![texpr_value, fexpr_value];
            //let mut blocks = vec![texpr_block, fexpr_block];

            //LLVMAddIncoming(phi, values.as_mut_ptr(), blocks.as_mut_ptr(), 2);
            //phi
            Box::new(Data(body_value))
        },

        AST::List(ref pos, ref items) => {
            // TODO this is kinda wrong, since you're passing it without type params
            let list = build_call(data, "List_new", &mut vec!(compile_node(data, func, unwind, scope.clone(), &AST::New(pos.clone(), (String::from("List"), vec!()))).get_ref()));
            for item in items {
                let value = build_cast_to_vartype(data, compile_node(data, func, unwind, scope.clone(), item).get_ref());
                build_call(data, "List_push", &mut vec!(list, value));
            }
            Box::new(Data(list))
        },


        AST::New(_, (ref name, ref types)) => {
            let classdef = scope.borrow().get_class_def(name);
            let value = classdef.borrow().search(&String::from("__alloc__"), |sym| sym.value.clone());
            let object = if let Some(function) = value {
                let mut largs = vec!();
                //LLVMBuildCall(data.builder, function, largs.as_mut_ptr(), largs.len() as u32, label("tmp"))
                function.invoke(data, unwind, largs)
            } else {
                //panic!("InternalError: no __alloc__ method for {}", name);
                let value = scope.borrow().get_type_value(name).unwrap();
                let mem = LLVMBuildMalloc(data.builder, LLVMGetElementType(value.value), label("ptr"));
                let object = LLVMBuildPointerCast(data.builder, mem, value.value, label("ptr"));
                if let Some(index) = value.structdef.iter().position(|ref r| r.0.as_str() == "__vtable__") {
                    let vtable = LLVMGetNamedGlobal(data.module, label(format!("__{}_vtable", name).as_str()));
                    let mut indices = vec!(i32_value(data, 0), i32_value(data, index));
                    let pointer = LLVMBuildGEP(data.builder, object, indices.as_mut_ptr(), indices.len() as u32, label("tmp"));
                    LLVMBuildStore(data.builder, vtable, pointer);
                }
                object
            };

            //let init = classdef.borrow().search(&String::from("__init__"), |sym| sym.value.clone());
            //let mut largs = vec!(object);
            //LLVMBuildCall(data.builder, init, largs.as_mut_ptr(), largs.len() as u32, label("tmp"));
            Box::new(Data(object))
        },

        AST::Class(_, (ref name, ref types), ref parent, ref body, ref id) => {
            //let tscope = data.map.get(id);
            //let classdef = scope.borrow().get_class_def(name);

            // TODO you still need to compile the body of the func, if you're going to allow that... like an init function
            //compile_vec(data, func, unwind, tscope.clone(), body);

            Box::new(Data(null_value(str_type(data))))
        },

        AST::Resolver(ref pos, ref left, ref right) => {
            match **left {
                AST::Identifier(_, ref name) => {
                    let classdef = scope.borrow().get_class_def(name);
                    let value = classdef.borrow().get_variable_value(right);
                    value.unwrap()
                },
                _ => panic!("SyntaxError:{:?}: left-hand side of scope resolver must be identifier", pos)
            }
        },

        AST::Accessor(_, ref left, ref right, ref ltype) => {
            let object = compile_node(data, func, unwind, scope.clone(), left).get_ref();

            let name = ltype.clone().unwrap().get_name().unwrap();
            let classdef = scope.borrow().get_class_def(&name);
            let classval = scope.borrow().get_type_value(&name).unwrap();
            debug!("*ACCESS: {:?} {:?}", right, classdef);

            let structindex = classval.structdef.iter().position(|ref r| r.0 == *right).unwrap_or(usize::max_value());
            let vtableindex = classval.vtable.iter().position(|ref r| r.0 == *right).unwrap_or(usize::max_value());

            if structindex != usize::max_value() {
                let pointer = build_struct_access(data, scope.clone(), object, &name, right);
                from_type(&classval.structdef[structindex].1, LLVMBuildLoad(data.builder, pointer, label("tmp")))
            } else if vtableindex != usize::max_value() {
                let vindex = classval.structdef.iter().position(|ref r| r.0.as_str() == "__vtable__").unwrap();
                let mut indices = vec!(i32_value(data, 0), i32_value(data, vindex));
                let pointer = LLVMBuildGEP(data.builder, object, indices.as_mut_ptr(), indices.len() as u32, label("tmp"));
                let vtable = LLVMBuildLoad(data.builder, pointer, label("tmp"));

                let mut indices = vec!(i32_value(data, 0), i32_value(data, vtableindex));
                let pointer = LLVMBuildGEP(data.builder, vtable, indices.as_mut_ptr(), indices.len() as u32, label("tmp"));
                from_type(&classval.vtable[vtableindex].1, LLVMBuildLoad(data.builder, pointer, label("tmp")))
            } else {
                let value = classdef.borrow().get_variable_value(right);
                value.unwrap()
            }
        },

        AST::Assignment(_, ref left, ref right) => {
            let value = compile_node(data, func, unwind, scope.clone(), right);
            match **left {
                AST::Accessor(_, ref left, ref right, ref ltype) => {
                    let name = ltype.clone().unwrap().get_name().unwrap();
                    let object = compile_node(data, func, unwind, scope.clone(), left).get_ref();
                    let pointer = build_struct_access(data, scope.clone(), object, &name, right);
                    LLVMBuildStore(data.builder, value.get_ref(), pointer)
                },
                _ => panic!("???"),
            };
            value
        },

        AST::Import(ref pos, ref name, _) => {
            let module_init_name = format!("init.{}", name);
            let ftype = Type::Function(vec!(), Box::new(Type::Object(String::from("Bool"), vec!())), ABI::Molten);
            if LLVMGetNamedFunction(data.module, label(module_init_name.as_str())).is_null() {
                let lftype = LLVMFunctionType(bool_type(data), &mut [].as_mut_ptr(), 0, false as i32);
                let function = LLVMAddFunction(data.module, label(module_init_name.as_str()), lftype);
                scope.borrow_mut().define(module_init_name.clone(), Some(ftype.clone()));
                scope.borrow_mut().assign(&module_init_name, Box::new(Function(function)));
            }
            compile_node(data, func, None, scope.clone(), &AST::Invoke(pos.clone(), Box::new(AST::Identifier(pos.clone(), String::from(module_init_name))), vec!(), Some(ftype)));
            Box::new(Data(ptr::null_mut()))
        },

        AST::Declare(_, _, _) => { Box::new(Data(zero_int(data))) },

        AST::Type(_, _, _) => panic!("NotImplementedError: not yet supported, {:?}", node),

        AST::Underscore |
        AST::Index(_, _, _, _) => panic!("InternalError: ast element shouldn't appear at this late phase: {:?}", node),
    }
}

unsafe fn collect_functions_vec<'a>(data: &mut LLVM<'a>, scope: ScopeRef<Value, TypeValue>, items: &'a Vec<AST>) -> Option<Value> {
    let mut last = None;
    for item in items {
        last = collect_functions_node(data, scope.clone(), item);
    }
    last
}

unsafe fn collect_functions_node<'a>(data: &mut LLVM<'a>, scope: ScopeRef<Value, TypeValue>, node: &'a AST) -> Option<Value> {
    match *node {
        AST::Function(ref pos, ref name, ref args, ref rtype, ref body, ref id, ref abi) => {
            let fscope = data.map.get(id);
            let fname = scope.borrow().get_full_name(name, id.clone());

            let ftype = get_type(data, scope.clone(), Type::Function(args.iter().map(|t| t.1.clone().unwrap()).collect(), Box::new(rtype.clone().unwrap()), abi.clone()), false);
            let function = LLVMAddFunction(data.module, label(fname.as_str()), ftype);
            //LLVMSetGC(function, label("shadow-stack"));
            //LLVMSetPersonalityFn(function, LLVMGetNamedFunction(data.module, label("__gxx_personality_v0")));

            let dscope = Scope::target(scope.clone());
            match *name {
                Some(ref name) => dscope.borrow_mut().assign(name, from_abi(abi, function)),
                _ => { },
            }

            let nargs = LLVMCountParams(function) as usize;
            if nargs != 0 && nargs != args.len() {
                panic!("ArgsError: argument counts don't match");
            }

            for (i, &(ref name, ref ttype, _)) in args.iter().enumerate() {
                let llarg = LLVMGetParam(function, i as u32);
                LLVMSetValueName(llarg, label(name.as_str()));
                fscope.borrow_mut().assign(name, from_type(ttype.as_ref().unwrap(), llarg));
            }

            collect_functions_node(data, fscope, body);
            data.functions.push(node);
            return Some(from_abi(abi, function));
        },

        AST::List(_, ref items) => { collect_functions_vec(data, scope, items); },

        AST::Invoke(_, _, ref args, _) => { collect_functions_vec(data, scope, args); },

        AST::SideEffect(_, _, ref args) => { collect_functions_vec(data, scope, args); },

        AST::Definition(_, (ref name, _), ref value) => {
            //collect_functions_node(data, scope, value);
            if let Some(function) = collect_functions_node(data, scope.clone(), value) {
                let dscope = Scope::target(scope.clone());
                dscope.borrow_mut().assign(name, function);
            }
        },

        AST::Declare(_, ref name, ref ttype) => {
            // TODO what to do about abi??
            if let &Type::Function(_, _, _) = ttype {
                let fname = scope.borrow().get_full_name(&Some(name.clone()), UniqueID(0));
                let function = LLVMAddFunction(data.module, label(fname.as_str()), get_type(data, scope.clone(), ttype.clone(), false));

                let dscope = Scope::target(scope.clone());
                dscope.borrow_mut().assign(name, from_abi(&ttype.get_abi().unwrap(), function));
            }
        },

        AST::Block(_, ref body) => { collect_functions_vec(data, scope, body); },

        AST::If(_, ref cond, ref texpr, ref fexpr) => {
            collect_functions_node(data, scope.clone(), cond);
            collect_functions_node(data, scope.clone(), texpr);
            collect_functions_node(data, scope, fexpr);
        },

        AST::Raise(_, ref expr) => { collect_functions_node(data, scope, expr); },

        AST::Try(_, ref cond, ref cases) |
        AST::Match(_, ref cond, ref cases) => {
            collect_functions_node(data, scope.clone(), cond);
            for case in cases {
                collect_functions_node(data, scope.clone(), &case.0);
                collect_functions_node(data, scope.clone(), &case.1);
            }
        },

        AST::For(_, _, ref cond, ref body, ref id) => {
            let lscope = data.map.get(id);
            collect_functions_node(data, lscope.clone(), cond);
            collect_functions_node(data, lscope.clone(), body);
        },

        AST::While(_, ref cond, ref body) => {
            collect_functions_node(data, scope.clone(), cond);
            collect_functions_node(data, scope.clone(), body);
        },

        AST::New(_, _) => { },

        AST::Class(ref pos, (ref name, ref types), ref parent, ref body, ref id) => {
            let tscope = data.map.get(id);

            let (mut structdef, mut vtable) = if let Some((ref name, ref types)) = *parent {
                let value = scope.borrow().get_type_value(name).unwrap();
                (value.structdef, value.vtable)
            } else {
                (vec!(), vec!())
            };

            // Build vtable for class
            let classdef = scope.borrow().get_class_def(name);
            let parent = classdef.borrow().get_parent().unwrap_or(Scope::new_ref(None));
if name.as_str() != "String" {
            for ref node in body.iter() {
                match **node {
                    AST::Function(_, ref fname, ref args, ref rtype, _, _, ref abi) => {
//if fname.clone().unwrap().as_str() != "new" {
                        if fname.is_some() && parent.borrow().contains(fname.as_ref().unwrap()) {
                            debug!("***************: {:?}:{:?}", name, fname);
                            vtable.push((fname.clone().unwrap(), Type::Function(args.iter().map(|t| t.1.clone().unwrap()).collect(), Box::new(rtype.clone().unwrap()), abi.clone())));
                        }
//}
                    },
                    AST::Declare(_, ref fname, ref ttype) => {
                        match *ttype {
                            Type::Function(_, _, _) => {
//if fname.as_str() != "new" {
                                if parent.borrow().contains(fname) {
                                    debug!("+++++++++++++++: {:?}:{:?}", name, fname);
                                    vtable.push((fname.clone(), ttype.clone()))
                                }
//}
                            },
                            _ => { },
                        }
                    },
                    _ => { }
                }
            }
}

            // Build struct definition for class
            if vtable.len() > 0 {
                if let Some(index) = structdef.iter().position(|ref r| r.0.as_str() == "__vtable__") {
                    structdef[index].1 = Type::Object(format!("{}_vtable", name), vec!());
                } else {
                    structdef.push((String::from("__vtable__"), Type::Object(format!("{}_vtable", name), vec!())));
                }
            }
            for ref node in body.iter() {
                match **node {
                    AST::Definition(_, (ref name, ref ttype), ref value) => {
                        structdef.push((name.clone(), ttype.clone().unwrap()));
                    },
                    _ => { }
                }
            }

            let lltype = build_class_type(data, scope.clone(), name, structdef, vtable);

            //let alloc = String::from("__alloc__");
            //let classdef = scope.borrow().get_class_def(name);
            //if !classdef.borrow().contains_local(&alloc) {
            //    debug!("******* CREATING ALLOC: {}", name);
            //    let cname = scope.borrow().get_full_name(&Some(name.clone()), id.clone());
            //    classdef.borrow_mut().define(alloc.clone(), Some(Type::Function(vec!(), Box::new(Type::Object(name.clone(), types.clone())))));
            //    classdef.borrow_mut().assign(&alloc, build_allocator(data, tscope.clone(), &name, format!("{}_{}", cname, alloc).as_str(), lltype));
            //}
            collect_functions_vec(data, tscope.clone(), body);
            data.classes.push(node);
        },

        AST::Index(_, ref left, ref right, _) => {
            collect_functions_node(data, scope.clone(), left);
            collect_functions_node(data, scope.clone(), right);
        },

        AST::Resolver(_, ref left, ref right) => {
            collect_functions_node(data, scope.clone(), left);
        },

        AST::Accessor(_, ref left, ref right, _) => {
            collect_functions_node(data, scope.clone(), left);
        },

        AST::Assignment(_, ref left, ref right) => {
            collect_functions_node(data, scope.clone(), right);
        },

        AST::Import(_, _, ref decls) => {
            collect_functions_vec(data, scope.clone(), decls);
        },

        AST::Type(_, _, _) => panic!("NotImplementedError: not yet supported, {:?}", node),

        AST::Identifier(_, _) |
        AST::Noop | AST::Underscore | AST::Nil(_) |
        AST::Boolean(_) | AST::Integer(_) | AST::Real(_) | AST::String(_) => { }
    };
    None
}


pub fn label(string: &str) -> *mut i8 {
    CString::new(string).unwrap().into_raw()
}

pub unsafe fn bool_type(data: &LLVM) -> LLVMTypeRef {
    LLVMInt1TypeInContext(data.context)
}

pub unsafe fn i32_type(data: &LLVM) -> LLVMTypeRef {
    LLVMInt32TypeInContext(data.context)
}

pub unsafe fn int_type(data: &LLVM) -> LLVMTypeRef {
    LLVMInt64TypeInContext(data.context)
}

pub unsafe fn real_type(data: &LLVM) -> LLVMTypeRef {
    LLVMDoubleTypeInContext(data.context)
}

pub unsafe fn str_type(data: &LLVM) -> LLVMTypeRef {
    LLVMPointerType(LLVMInt8TypeInContext(data.context), 0)
}

pub unsafe fn ptr_type(data: &LLVM) -> LLVMTypeRef {
    LLVMPointerType(LLVMPointerType(LLVMInt8TypeInContext(data.context), 0), 0)
}


pub unsafe fn null_value(ttype: LLVMTypeRef) -> LLVMValueRef {
    LLVMConstNull(ttype)
}

pub unsafe fn zero_int(data: &LLVM) -> LLVMValueRef {
    LLVMConstInt(int_type(data), 0, 0)
}

pub unsafe fn i32_value(data: &LLVM, num: usize) -> LLVMValueRef {
    LLVMConstInt(i32_type(data), num as u64, 0)
}

pub unsafe fn int_value(data: &LLVM, num: usize) -> LLVMValueRef {
    LLVMConstInt(int_type(data), num as u64, 0)
}

//pub unsafe fn build_str_const(data: &LLVM, string: &str) -> LLVMValueRef {
//    LLVMBuildGlobalStringPtr(data.builder, label(string), label("str"))
//}

pub unsafe fn build_generic_cast(data: &LLVM, value: LLVMValueRef, ltype: LLVMTypeRef) -> LLVMValueRef {
    if LLVMGetTypeKind(LLVMTypeOf(value)) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
        if LLVMGetTypeKind(ltype) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
            LLVMBuildPointerCast(data.builder, value, ltype, label("ptr"))
        } else {
            LLVMBuildPtrToInt(data.builder, value, ltype, label("ptr"))
        }
    } else {
        if LLVMGetTypeKind(ltype) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
            LLVMBuildIntToPtr(data.builder, value, ltype, label("ptr"))
        } else {
            panic!("I HAVEN'T DONE THIS");
        }
    }
}

pub unsafe fn build_cast_to_vartype(data: &LLVM, value: LLVMValueRef) -> LLVMValueRef {
    if LLVMGetTypeKind(LLVMTypeOf(value)) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
        LLVMBuildPointerCast(data.builder, value, str_type(data), label("ptr"))
    } else {
        LLVMBuildIntToPtr(data.builder, value, str_type(data), label("ptr"))
    }
}

pub unsafe fn build_cast_from_vartype(data: &LLVM, value: LLVMValueRef, ltype: LLVMTypeRef) -> LLVMValueRef {
    if LLVMGetTypeKind(ltype) == llvm::LLVMTypeKind::LLVMPointerTypeKind {
        LLVMBuildPointerCast(data.builder, value, ltype, label("ptr"))
    } else {
        LLVMBuildPtrToInt(data.builder, value, ltype, label("ptr"))
    }
}


//pub unsafe fn get_attribute(data: &LLVM, name: &str) -> LLVMAttributeRef {
//    let kind = LLVMGetEnumAttributeKindForName(label(name), name.len());
//    LLVMCreateEnumAttribute(data.context, kind, 0)
//}

pub unsafe fn build_function_start(data: &LLVM, name: &str, mut args: Vec<LLVMTypeRef>, return_type: LLVMTypeRef) -> LLVMValueRef {
    let ftype = LLVMFunctionType(return_type, args.as_mut_ptr(), args.len() as u32, false as i32);
    let function = LLVMAddFunction(data.module, label(name), ftype);

    let nargs = LLVMCountParams(function) as usize;
    if nargs != 0 && nargs != args.len() {
        panic!("ArgsError: argument counts don't match");
    }

    // TODO maybe these shouldn't be here, but it causes problems for library functions without it
    let bb = LLVMAppendBasicBlockInContext(data.context, function, label("entry"));
    LLVMPositionBuilderAtEnd(data.builder, bb);

    function
}

unsafe fn build_function_body(data: &LLVM, node: &AST) {
    if let AST::Function(_, ref name, _, _, ref body, ref id, ref abi) = *node {
        // TODO do you need to take into account abi?
        let fscope = data.map.get(id);
        let pscope = fscope.borrow().parent.clone().unwrap();
        let fname = pscope.borrow().get_full_name(name, id.clone());
        let function = LLVMGetNamedFunction(data.module, label(fname.as_str()));

        let bb = LLVMAppendBasicBlockInContext(data.context, function, label("entry"));
        LLVMPositionBuilderAtEnd(data.builder, bb);
        let ret = compile_node(data, function, None, fscope.clone(), body);
        LLVMBuildRet(data.builder, ret.get_ref());

        //if llvm::analysis::LLVMVerifyFunction(function, llvm::analysis::LLVMVerifierFailureAction::LLVMPrintMessageAction) != 0 {
        //    panic!("VerifyError: verification failed");
        //}
        //LLVMRunFunctionPassManager(data.funcpass, function);
    }
}

pub unsafe fn build_call(data: &LLVM, name: &str, largs: &mut Vec<LLVMValueRef>) -> LLVMValueRef {
    let function = LLVMGetNamedFunction(data.module, label(name));
    LLVMBuildCall(data.builder, function, largs.as_mut_ptr(), largs.len() as u32, label("tmp"))
}

/*
pub unsafe fn build_allocator(data: &LLVM, scope: ScopeRef<Value, TypeValue>, cname: &String, fname: &str, lltype: LLVMTypeRef) -> LLVMValueRef {
    let function = build_function_start(data, fname, vec!(), lltype);
    //let obj = build_malloc(data, LLVMSizeOf(LLVMGetElementType(lltype)));
    let mem = LLVMBuildMalloc(data.builder, LLVMGetElementType(lltype), label("ptr"));
    let object = LLVMBuildPointerCast(data.builder, mem, lltype, label("ptr"));

    let structdef = scope.borrow().get_type_value(&cname).unwrap().structdef;
    //for &(name, ttype, expr) in &structdef {
    //    let value = compile_node(data, function, None, scope.clone(), &expr);
    //    let pointer = build_struct_access(data, scope.clone(), object, cname, &name);
    //    LLVMBuildStore(data.builder, pointer, value);
    //}

    LLVMBuildRet(data.builder, object);
    function
}
*/

pub unsafe fn vtable_has_member(scope: ScopeRef<Value, TypeValue>, typename: &String, field: &String) -> bool {
    scope.borrow().search_type(typename, |info| {
        if info.value.as_ref().unwrap().vtable.iter().position(|ref r| r.0 == *field).is_some() {
            Some(true)
        } else {
            Some(false)
        }
    }).unwrap_or(false)
}

pub unsafe fn struct_has_member(scope: ScopeRef<Value, TypeValue>, typename: &String, field: &String) -> bool {
    scope.borrow().search_type(typename, |info| {
        if info.value.as_ref().unwrap().structdef.iter().position(|ref r| r.0 == *field).is_some() {
            Some(true)
        } else {
            Some(false)
        }
    }).unwrap_or(false)
}

pub unsafe fn build_struct_access(data: &LLVM, scope: ScopeRef<Value, TypeValue>, object: LLVMValueRef, typename: &String, field: &String) -> LLVMValueRef {
    let structdef = scope.borrow().get_type_value(&typename).unwrap().structdef;
    let index = structdef.iter().position(|ref r| r.0 == *field).unwrap();
    let mut indices = vec!(i32_value(data, 0), i32_value(data, index));
    LLVMBuildGEP(data.builder, object, indices.as_mut_ptr(), indices.len() as u32, label("tmp"))
}

pub unsafe fn build_class_type(data: &LLVM, scope: ScopeRef<Value, TypeValue>, name: &String, structdef: Vec<(String, Type)>, vtable: Vec<(String, Type)>) -> LLVMTypeRef {
    let (vttype, pvttype) = if vtable.len() > 0 {
        let vtname = format!("{}_vtable", name);
        let vttype = LLVMStructCreateNamed(data.context, label(vtname.as_str()));
        let pvttype = LLVMPointerType(vttype, 0);
        scope.borrow_mut().define_type(vtname.clone(), Type::Object(vtname.clone(), vec!()));
        scope.borrow_mut().set_type_value(&vtname, TypeValue { structdef: vtable.clone(), value: pvttype, vtable: vec!(), vttype: None });
        (Some(vttype), Some(pvttype))
    } else {
        (None, None)
    };
    let lltype = LLVMStructCreateNamed(data.context, label(name));
    let pltype = LLVMPointerType(lltype, 0);
    scope.borrow_mut().set_type_value(name, TypeValue { structdef: structdef.clone(), value: pltype, vtable: vtable.clone(), vttype: pvttype });

    let mut types = vec!();
    for &(_, ref ttype) in &structdef {
        types.push(get_type(data, scope.clone(), ttype.clone(), true))
    }
    LLVMStructSetBody(lltype, types.as_mut_ptr(), types.len() as u32, false as i32);

    if let Some(vttype) = vttype {
        let mut types = vec!();
        for &(_, ref ttype) in &vtable {
            types.push(get_type(data, scope.clone(), ttype.clone(), true))
        }
        LLVMStructSetBody(vttype, types.as_mut_ptr(), types.len() as u32, false as i32);
    }

    pltype
}

pub unsafe fn get_type(data: &LLVM, scope: ScopeRef<Value, TypeValue>, ttype: Type, use_fptrs: bool) -> LLVMTypeRef {
    match ttype {
        Type::Object(ref tname, ref ptypes) => match tname.as_str() {
            "Nil" => str_type(data),
            "Bool" => bool_type(data),
            "Byte" => LLVMInt8TypeInContext(data.context),
            "Int" => int_type(data),
            "Real" => real_type(data),
            "String" => str_type(data),
            "Buffer" => ptr_type(data),

            _ => match scope.borrow().get_type_value(tname) {
                Some(typedata) => typedata.value,
                // TODO this should panic...  but Nil doesn't have a value (because it needs to know the type of null pointer it should be)
                //None => LLVMInt64TypeInContext(data.context),
                None => panic!("CompileError: unassigned type value, {:?}", tname),
            }
        },
        Type::Function(ref args, ref ret, _) => {
            // TODO should you incorporate abi??
            let mut atypes = vec!();
            for ttype in args {
                atypes.push(get_type(data, scope.clone(), ttype.clone(), true));
            }
            let rtype = get_type(data, scope.clone(), *ret.clone(), true);
            let ftype = LLVMFunctionType(rtype, atypes.as_mut_ptr(), atypes.len() as u32, false as i32);
            if use_fptrs {
                LLVMPointerType(ftype, 0)
            } else {
                ftype
            }
        },
        // TODO this is not the correct way to deal with type variables... there should be overloaded functions generated
        //Type::Variable(_, _) => LLVMInt64TypeInContext(data.context),
        Type::Variable(_, _) => str_type(data), //ptr_type(data),
        _ => panic!("InvalidType: cannot convert to llvm, {:?}", ttype),
    }
}
