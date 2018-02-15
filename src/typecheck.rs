
use std::fmt::Debug;

use parser::AST;
use types::{ Type, Check, expect_type, resolve_type, find_variant, check_type_params };
use scope::{ self, Scope, ScopeRef, ScopeMapRef };


pub fn check_types<V, T>(map: ScopeMapRef<V, T>, scope: ScopeRef<V, T>, code: &mut Vec<AST>) -> Type where V: Clone + Debug, T: Clone + Debug {
    let mut last: Type = Type::Object(String::from("Nil"), vec!());
    for node in code {
        last = check_types_node(map.clone(), scope.clone(), node, None);
    }
    last
}

pub fn check_types_node<V, T>(map: ScopeMapRef<V, T>, scope: ScopeRef<V, T>, node: &mut AST, expected: Option<Type>) -> Type where V: Clone + Debug, T: Clone + Debug {
    let x = match *node {
        AST::Boolean(_) => Type::Object(String::from("Bool"), vec!()),
        AST::Integer(_) => Type::Object(String::from("Int"), vec!()),
        AST::Real(_) => Type::Object(String::from("Real"), vec!()),
        AST::String(_) => Type::Object(String::from("String"), vec!()),

        AST::Function(ref mut name, ref mut args, ref mut rtype, ref mut body, ref id) => {
            let fscope = map.get(id);

            let mut argtypes = vec!();
            for &(ref name, ref ttype, ref value) in &*args {
                let vtype = value.clone().map(|ref mut vexpr| check_types_node(map.clone(), scope.clone(), vexpr, ttype.clone()));
                let mut atype = expect_type(fscope.clone(), ttype.clone(), vtype, Check::Def);
                if &name[..] == "self" {
                    let mut stype = fscope.borrow().find_type(&String::from("Self")).unwrap();
                    stype = fscope.borrow_mut().map_all_typevars(stype);
                    atype = expect_type(fscope.clone(), Some(atype), Some(stype), Check::Def);
                }
                fscope.borrow_mut().update_variable_type(name, atype.clone());
                //Type::update_variable_type(fscope.clone(), name, atype.clone());
                argtypes.push(atype);
            }

            let rettype = expect_type(fscope.clone(), rtype.clone(), Some(check_types_node(map.clone(), fscope.clone(), body, rtype.clone())), Check::Def);
            *rtype = Some(rettype.clone());

            // Resolve type variables that can be
            for i in 0 .. argtypes.len() {
                argtypes[i] = resolve_type(fscope.clone(), argtypes[i].clone());
                // TODO this is still not checking for type compatibility
                fscope.borrow_mut().update_variable_type(&args[i].0, argtypes[i].clone());
                //Type::update_variable_type(fscope.clone(), &args[i].0, argtypes[i].clone());
                args[i].1 = Some(argtypes[i].clone());
            }
            update_scope_variable_types(fscope.clone());
            // TODO this fixes the type error, but makes a ton of stupid typevars
            //scope.borrow_mut().raise_types(fscope.clone());

            let mut nftype = Type::Function(argtypes.clone(), Box::new(rettype));
            if name.is_some() {
                let dscope = Scope::target(scope.clone());
                let dname = name.clone().unwrap();
                if dscope.borrow().is_overloaded(&dname) {
                    let fname = scope::mangle_name(&dname, &argtypes);
                    dscope.borrow_mut().define(fname.clone(), Some(nftype.clone()));
                    *name = Some(fname);
                }

                //let mut otype = dscope.borrow().get_variable_type(&dname);
                //nftype = otype.map(|ttype| ttype.add_variant(scope.clone(), nftype.clone())).unwrap_or(nftype);
                //dscope.borrow_mut().update_variable_type(&dname, nftype.clone());
                Scope::add_func_variant(dscope.clone(), &dname, scope.clone(), nftype.clone());
            }

            //println!("RAISING: {:?} {:?} {:?}", scope.borrow().get_basename(), fscope.borrow().get_basename(), nftype);
            scope.borrow_mut().raise_type(fscope.clone(), nftype.clone());
            nftype
        },

        AST::Invoke(ref mut fexpr, ref mut args, ref mut stype) => {
            let mut atypes = vec!();
            for ref mut value in args {
                atypes.push(check_types_node(map.clone(), scope.clone(), value, None));
            }

            let tscope = Scope::new_ref(Some(scope.clone()));
            let mut etype = check_types_node(map.clone(), scope.clone(), fexpr, None);
            etype = tscope.borrow_mut().map_all_typevars(etype.clone());

            if let Type::Overload(_) = etype {
                etype = find_variant(tscope.clone(), etype, atypes.clone());
                match **fexpr {
                    AST::Resolver(_, ref mut name) |
                    AST::Accessor(_, ref mut name, _) |
                    AST::Identifier(ref mut name) => *name = scope::mangle_name(name, etype.get_argtypes()),
                    _ => panic!("OverloadError: call to overloaded method not allowed here"),
                }
            }

            let ftype = match etype {
                Type::Function(_, _) => {
                    let ftype = expect_type(tscope.clone(), Some(etype.clone()), Some(Type::Function(atypes, Box::new(etype.get_rettype().clone()))), Check::Def);
                    // TODO should this actually be another expect, so type resolutions that occur in later args affect earlier args?  Might not be needed unless you add typevar constraints
                    let ftype = resolve_type(tscope.clone(), ftype);        // NOTE This ensures the early arguments are resolved despite typevars not being assigned until later in the signature

                    ftype
                },
                Type::Variable(ref name) => {
                    let ftype = Type::Function(atypes.clone(), Box::new(expected.unwrap_or_else(|| tscope.borrow_mut().new_typevar())));
                    // TODO This is suspect... we might be updating type without checking for a conflict
                    // TODO we also aren't handling other function types, like accessor and resolve
                    //if let AST::Identifier(ref fname) = **fexpr {
                    //    update_type(scope.clone(), fname, ftype.clone());
                    //}
                    //tscope.borrow_mut().update_type(name, ftype.clone());
                    Type::update_type(tscope.clone(), name, ftype.clone());
                    ftype
                },
                _ => panic!("Not a function: {:?}", fexpr),
            };

            scope.borrow_mut().raise_types(tscope.clone());
            *stype = Some(ftype.clone());
            ftype.get_rettype().clone()
        },

        AST::SideEffect(_, ref mut args) => {
            let mut ltype = None;
            for ref mut expr in args {
                ltype = Some(expect_type(scope.clone(), ltype.clone(), Some(check_types_node(map.clone(), scope.clone(), expr, ltype.clone())), Check::List));
            }
            ltype.unwrap()
        },

        AST::Definition((ref name, ref mut ttype), ref mut body) => {
            let dscope = Scope::target(scope.clone());
            let btype = expect_type(scope.clone(), ttype.clone(), Some(check_types_node(map.clone(), scope.clone(), body, ttype.clone())), Check::Def);
            dscope.borrow_mut().update_variable_type(name, btype.clone());
            //Type::update_variable_type(dscope.clone(), name, btype.clone());
            *ttype = Some(btype.clone());
            btype
        },

        AST::Declare(ref name, ref ttype) => {
            //let dscope = Scope::target(scope.clone());
            //dscope.borrow_mut().update_variable_type(name, ttype.clone());
            ttype.clone()
        },

        AST::Identifier(ref name) => {
            let mut bscope = scope.borrow_mut();
            bscope.get_variable_type(name).unwrap_or_else(|| expected.unwrap_or_else(|| bscope.new_typevar()))
        },

        AST::Block(ref mut body) => check_types(map, scope, body),

        AST::If(ref mut cond, ref mut texpr, ref mut fexpr) => {
            // TODO should this require the cond type to be Bool?
            check_types_node(map.clone(), scope.clone(), cond, None);
            let ttype = check_types_node(map.clone(), scope.clone(), texpr, None);
            let ftype = check_types_node(map.clone(), scope.clone(), fexpr, Some(ttype.clone()));
            expect_type(scope.clone(), Some(ttype), Some(ftype), Check::List)
        },

        AST::Try(ref mut cond, ref mut cases) |
        AST::Match(ref mut cond, ref mut cases) => {
            let mut ctype = Some(check_types_node(map.clone(), scope.clone(), cond, None));
            let mut rtype = None;
            for &mut (ref mut case, ref mut expr) in cases {
                ctype = Some(expect_type(scope.clone(), ctype.clone(), Some(check_types_node(map.clone(), scope.clone(), case, ctype.clone())), Check::List));
                rtype = Some(expect_type(scope.clone(), rtype.clone(), Some(check_types_node(map.clone(), scope.clone(), expr, rtype.clone())), Check::List));
            }
            rtype.unwrap()
        },

        AST::Raise(ref mut expr) => {
            // TODO should you check for a special error/exception type?
            check_types_node(map.clone(), scope.clone(), expr, None)
        },

        AST::While(ref mut cond, ref mut body) => {
            // TODO should this require the cond type to be Bool?
            check_types_node(map.clone(), scope.clone(), cond, None);
            check_types_node(map.clone(), scope.clone(), body, None);
            Type::Object(String::from("Nil"), vec!())
        },

        AST::For(ref name, ref mut list, ref mut body, ref id) => {
            let lscope = map.get(id);
            let itype = lscope.borrow().get_variable_type(name).unwrap_or_else(|| expected.unwrap_or_else(|| scope.borrow_mut().new_typevar()));
            let etype = Some(Type::Object(String::from("List"), vec!(itype)));
            let ltype = expect_type(lscope.clone(), etype.clone(), Some(check_types_node(map.clone(), lscope.clone(), list, etype.clone())), Check::Def);
            lscope.borrow_mut().update_variable_type(name, ltype.get_params()[0].clone());
            //Type::update_variable_type(lscope.clone(), name, ltype);
            check_types_node(map.clone(), lscope.clone(), body, None)
        },

        AST::Nil(ref mut ttype) => {
            *ttype = Some(expected.unwrap_or_else(|| scope.borrow_mut().new_typevar()));
            ttype.clone().unwrap()
        },

        AST::List(ref mut items) => {
            let mut ltype = None;
            for ref mut expr in items {
                ltype = Some(expect_type(scope.clone(), ltype.clone(), Some(check_types_node(map.clone(), scope.clone(), expr, ltype.clone())), Check::List));
            }
            Type::Object(String::from("List"), vec!(ltype.unwrap_or_else(|| expected.unwrap_or_else(|| scope.borrow_mut().new_typevar()))))
        },

        AST::New((ref name, ref types)) => {
            let odtype = scope.borrow().find_type(name);
            match odtype {
                Some(dtype) => {
                    let tscope = Scope::new_ref(Some(scope.clone()));
                    let mtype = tscope.borrow_mut().map_all_typevars(dtype.clone());
                    if let Err(msg) = check_type_params(tscope.clone(), &mtype.get_params(), types, Check::Def, false) {
                        panic!(msg);
                    }
                    scope.borrow_mut().raise_types(tscope.clone());
                },
                None => panic!("TypeError: undefined type {:?}", name),
            };
            Type::Object(name.clone(), types.clone())
        },

        AST::Class(_, _, ref mut body, ref id) => {
            let tscope = map.get(id);
            check_types(map.clone(), tscope.clone(), body);
            Type::Object(String::from("Nil"), vec!())
        },

        AST::Resolver(ref mut left, ref mut field) => {
            let ltype = match **left {
                // TODO this caused an issue with types that have typevars that aren't declared (ie. Buffer['item])
                //AST::Identifier(ref name) => resolve_type(scope.clone(), scope.borrow().find_type(name).unwrap().clone()),
                AST::Identifier(ref name) => scope.borrow().find_type(name).unwrap().clone(),
                _ => panic!("SyntaxError: left-hand side of scope resolver must be identifier")
            };

            let classdef = scope.borrow().get_class_def(&ltype.get_name());
            let mut cborrow = classdef.borrow_mut();
            cborrow.get_variable_type(field).unwrap_or_else(|| expected.unwrap_or_else(|| scope.borrow_mut().new_typevar()))
        },

        AST::Accessor(ref mut left, ref mut field, ref mut stype) => {
            let ltype = resolve_type(scope.clone(), check_types_node(map.clone(), scope.clone(), left, None));
            *stype = Some(ltype.clone());

            let classdef = scope.borrow().get_class_def(&ltype.get_name());
            let mut cborrow = classdef.borrow_mut();
            cborrow.get_variable_type(field).unwrap_or_else(|| expected.unwrap_or_else(|| scope.borrow_mut().new_typevar()))
        },

        AST::Assignment(ref mut left, ref mut right) => {
            let ltype = check_types_node(map.clone(), scope.clone(), left, None);
            let rtype = check_types_node(map.clone(), scope.clone(), right, Some(ltype.clone()));
            expect_type(scope.clone(), Some(ltype), Some(rtype), Check::Def)
        },

        AST::Import(_, ref mut decls) => {
            check_types(map.clone(), scope.clone(), decls);
            Type::Object(String::from("Nil"), vec!())
        },

        AST::Noop => Type::Object(String::from("Nil"), vec!()),

        AST::Underscore => expected.unwrap_or_else(|| scope.borrow_mut().new_typevar()),

        AST::Type(_, _) => panic!("NotImplementedError: not yet supported, {:?}", node),

        AST::Index(_, _, _) => panic!("InternalError: ast element shouldn't appear at this late phase: {:?}", node),
    };
    
    println!("CHECK: {:?} {:?}", x, node);
    x
}


pub fn update_scope_variable_types<V, T>(scope: ScopeRef<V, T>) where V: Clone, T: Clone {
    let dscope = Scope::target(scope.clone());
    let mut names = vec!();
    for name in dscope.borrow().names.keys() {
        names.push(name.clone());
    }

    for name in &names {
        let otype = dscope.borrow_mut().get_variable_type(name).unwrap().clone();
        let ntype = resolve_type(scope.clone(), otype);
        dscope.borrow_mut().update_variable_type(name, ntype);
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use parser::*;
    use scope::*;

    #[test]
    fn basic_types() {
        assert_eq!(
            typecheck("5 * 3 + 8 / 100".as_bytes()),
            parse_type("Int").unwrap()
        );
        assert_eq!(
            typecheck("let x = 2 + 4 * 7 - 1 / 20  == 10 - 50 * 12".as_bytes()),
            parse_type("Bool").unwrap()
        );
    }

    #[test]
    fn function_types() {
        assert_eq!(
            typecheck("fn x, y -> { let a = x * 1.0; y * y }".as_bytes()),
            parse_type("(Real, 'e) -> 'e").unwrap()
        );
    }

    #[test]
    #[should_panic]
    fn type_errors_basic() {
        typecheck("5 * 3.0 + 8 / 100".as_bytes());
    }

    #[test]
    #[should_panic]
    fn type_errors_mismatch() {
        typecheck("let b : Bool = 123.24".as_bytes());
    }

    fn typecheck(text: &[u8]) -> Type {
        let result = parse(text);
        let mut code = result.unwrap().1;
        let map: ScopeMapRef<()> = bind_names(&mut code);
        check_types(map.clone(), map.get_global(), &mut code)
    }
}


