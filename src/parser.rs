

extern crate nom;
use nom::{ digit, hex_digit, oct_digit, line_ending, not_line_ending, space, multispace, is_alphanumeric, is_alphabetic, is_space, Needed, IResult, Context };
use nom::types::CompleteByteSlice;

extern crate nom_locate;
use nom_locate::LocatedSpan;

use std;
use std::f64;
use std::str;
use std::str::FromStr;

use abi::ABI;
use types::Type;
use utils::UniqueID;
use ast::{ Pos, Literal, Ident, Argument, ClassSpec, Field, AST };


///// Parsing Macros /////

pub type Span<'a> = LocatedSpan<CompleteByteSlice<'a>>;

#[inline]
pub fn span_to_string(s: Span) -> String {
    String::from(str::from_utf8(&s.fragment).unwrap())
}

//named!(sp, eat_separator!(&b" \t"[..]));

#[macro_export]
macro_rules! sp {
    ($i:expr, $($args:tt)*) => {
        sep!($i, sp, $($args)*)
    }
}

#[macro_export]
macro_rules! wscom {
    ($i:expr, $submac:ident!( $($args:tt)* )) => ({
        use parser::multispace_comment;
        //terminated!($i, $submac!($($args)*), multispace_comment)
        //preceded!($i, multispace_comment, $submac!($($args)*))
        //sep!($i, multispace_comment, $submac!($($args)*))
        delimited!($i, multispace_comment, $submac!($($args)*), multispace_comment)
    });

    ($i:expr, $f:expr) => (
        wscom!($i, call!($f));
    );
}

#[macro_export]
macro_rules! wscoml {
    ($i:expr, $submac:ident!( $($args:tt)* )) => ({
        use parser::multispace_comment;
        //terminated!($i, $submac!($($args)*), multispace_comment)
        preceded!($i, multispace_comment, $submac!($($args)*))
        //sep!($i, multispace_comment, $submac!($($args)*))
        //delimited!($i, multispace_comment, $submac!($($args)*), multispace_comment)
    });

    ($i:expr, $f:expr) => (
        wscom!($i, call!($f));
    );
}

#[macro_export]
macro_rules! tag_word (
    ($i:expr, $f:expr) => {
        do_parse!($i,
            s: tag!($f) >>
            not!(peek!(alphanumeric_underscore)) >>
            ( s )
        )
    }
);

#[macro_export]
macro_rules! map_ident (
    ($i:expr, $($args:tt)*) => {
        map!($i, $($args)*, |s| Ident::from_span(s))
    }
);


///// Parser /////

pub fn parse_or_error(name: &str, text: &[u8]) -> Vec<AST> {
    let span = Span::new(CompleteByteSlice(text));
    match parse(span) {
        Ok((rem, _)) if rem.fragment != CompleteByteSlice(&[]) => panic!("InternalError: unparsed input remaining: {:?}", rem),
        Ok((_, code)) => code,
        Err(err) => { print_error(name, span, err); panic!("") },
    }
}


named!(pub parse(Span) -> Vec<AST>,
    complete!(do_parse!(
        e: many0!(statement) >>
        eof!() >>
        (e)
    ))
);

named!(statement(Span) -> AST,
    //separated_list_complete!(ws!(tag!(",")), do_parse!(
    do_parse!(
        s: wscom!(alt_complete!(
            import |
            definition |
            assignment |
            whileloop |
            class |
            typedef |
            expression
            // TODO should class be here too?
        )) >>
        //eat_separator!("\n") >>
        separator >>
        (s)
    )
);

named!(import(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("import")) >>
        e: recognize!(separated_list_complete!(tag!("."), identifier)) >>
        (AST::make_import(Pos::new(pos), Ident::from_span(e), vec!()))
    )
);

named!(definition(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("let")) >>
        i: identifier_typed >>
        e: opt!(preceded!(
            wscom!(tag!("=")),
            expression
        )) >>
        (AST::make_def(Pos::new(pos), i.1, i.2, if e.is_some() { e.unwrap() } else { AST::make_nil() }))
    )
);

named!(assignment(Span) -> AST,
    do_parse!(
        pos: position!() >>
        o: subatomic_operation >>
        wscom!(tag!("=")) >>
        e: expression >>
        (AST::make_assign(Pos::new(pos), o, e))
    )
);

named!(whileloop(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("while")) >>
        c: expression >>
        opt!(multispace_comment) >>
        e: expression >>
        (AST::make_while(Pos::new(pos), c, e))
    )
);

named!(class(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("class")) >>
        i: class_spec >>
        p: opt!(preceded!(wscom!(tag_word!("extends")), class_spec)) >>
        wscom!(tag!("{")) >>
        s: many0!(alt_complete!(
                typedef |
                definition |
                declare |
                function
                )) >>
        wscom!(tag!("}")) >>
        (AST::make_class(Pos::new(pos), i, p, s))
        )
    );

named!(typedef(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("type")) >>
        c: class_spec >>
        wscom!(tag!("=")) >>
        ts: alt!(
            map!(identifier_typed, |i| vec!(i)) |
            delimited!(wscom!(tag!("{")), separated_list_complete!(wscom!(tag!(",")), identifier_typed), wscom!(tag!("}")))
        ) >>
        (AST::make_typedef(Pos::new(pos), c, ts.into_iter().map(|f| Field::new(f.0, f.1, f.2)).collect()))
    )
);



named!(expression(Span) -> AST,
    alt_complete!(
        underscore |
        //block |
        ifexpr |
        trywith |
        raise |
        matchcase |
        forloop |
        newclass |
        declare |
        function |
        infix
    )
);



named!(underscore(Span) -> AST,
    value!(AST::Underscore, tag!("_"))
);

named!(block(Span) -> AST,
    delimited!(
        wscom!(alt!(tag_word!("begin") | tag!("{"))),
        do_parse!(
            pos: position!() >>
            s: many0!(statement) >>
            (AST::make_block(Pos::new(pos), s))
        ),
        wscom!(alt!(tag_word!("end") | tag!("}")))
    )
);

named!(ifexpr(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("if")) >>
        c: expression >>
        wscom!(tag_word!("then")) >>
        t: expression >>
        f: opt!(preceded!(
            wscom!(tag_word!("else")),
            expression
        )) >>
        (AST::make_if(Pos::new(pos), c, t, if f.is_some() { f.unwrap() } else { AST::make_nil() }))
    )
);

named!(trywith(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("try")) >>
        c: expression >>
        wscom!(tag_word!("with")) >>
        l: caselist >>
        (AST::make_try(Pos::new(pos), c, l))
    )
);

named!(raise(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("raise")) >>
        e: expression >>
        (AST::make_raise(Pos::new(pos), e))
    )
);

named!(matchcase(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("match")) >>
        c: expression >>
        //wscom!(tag_word!("with")) >>
        //l: caselist >>
        l: delimited!(wscom!(tag!("{")), caselist, wscom!(tag!("}"))) >>
        (AST::make_match(Pos::new(pos), c, l))
    )
);

named!(caselist(Span) -> Vec<(AST, AST)>,
    //separated_list_complete!(wscom!(tag!(",")), do_parse!(
    many1!(do_parse!(
        //wscom!(tag!("|")) >>
        c: alt_complete!(value!(AST::Underscore, tag!("_")) | literal) >>
        wscom!(tag!("=>")) >>
        e: expression >>
        //wscom!(tag!(",")) >>
        (c, e)
    ))
);

named!(forloop(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("for")) >>
        i: identifier >>
        wscom!(tag_word!("in")) >>
        l: expression >>
        opt!(multispace_comment) >>
        e: expression >>
        (AST::make_for(Pos::new(pos), i, l, e))
    )
);

named!(newclass(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("new")) >>
        cs: class_spec >>
        a: map!(
            delimited!(tag!("("), expression_list, tag!(")")),
            |mut a| { a.insert(0, AST::make_new(Pos::new(pos), cs.clone())); a }
        ) >>
        (AST::PtrCast(
            Type::Object(cs.ident.name.clone(), UniqueID(0), cs.types.clone()),
            Box::new(AST::make_invoke(Pos::new(pos), AST::make_resolve(Pos::new(pos), AST::make_ident(Pos::new(pos), cs.ident.clone()), Ident::from_str("new")), a))))
    )
);

named!(declare(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("decl")) >>
        n: symbol_name >>
        wscom!(tag!(":")) >>
        t: type_description >>
        (AST::make_decl(Pos::new(pos), n, t))
    )
);

named!(function(Span) -> AST,
    do_parse!(
        pos: position!() >>
        wscom!(tag_word!("fn")) >>
        //n: opt!(identifier) >>
        l: alt_complete!(
            do_parse!(
                n: opt!(alt_complete!(identifier | any_op)) >>
                //n: opt!(identifier) >>
                l: delimited!(tag!("("), argument_list, tag!(")")) >>
                ((n, l))
            ) |
            map!(argument_list, |l| (None, l))
        ) >>
        r: opt!(preceded!(wscom!(tag!("->")), type_description)) >>
        a: abi_specifier >>
        e: alt_complete!(block | preceded!(wscom!(tag!("=>")), expression)) >>
        (AST::make_func(Pos::new(pos), l.0, l.1, r, e, a))
    )
);

named!(argument_list(Span) -> Vec<Argument>,
    separated_list_complete!(tag!(","),
        do_parse!(
            i: identifier_typed >>
            d: opt!(preceded!(tag!("="), expression)) >>
            (Argument::new(i.0, i.1, i.2, d))
        )
    )
);


named!(infix_op(Span) -> Ident,
    map_ident!(
        alt!(
            tag!("*") |
            tag!("/") |
            tag!("^") |
            tag!("%") |
            tag!("+") |
            tag!("-") |
            tag!("<<") |
            tag!(">>") |
            tag!("<=") |
            tag!(">=") |
            tag!("<") |
            tag!(">") |
            tag!("==") |
            tag!("!=") |
            tag!("&") |
            tag!("|") |
            tag_word!("and") |
            tag_word!("or")
            //tag!("..") |
        )
        //tag!(".")
    )
);

impl AST {
    fn precedence(op: &str) -> i32 {
        match op {
            "*" | "/" | "%"         => 5,
            "+" | "-"               => 6,
            "<<" | ">>"             => 7,
            "<" | ">" | "<=" | ">=" => 8,
            "==" | "!="             => 9,
            "&"                     => 10,
            "|"                     => 12,
            "and"                   => 13,
            "or"                    => 14,
            _                       => 20,
        }
    }

    fn fold_op(left: AST, operations: Vec<(Span, Ident, AST)>) -> Self {
        let mut operands: Vec<AST> = vec!();
        let mut operators: Vec<(Pos, Ident, i32)> = vec!();
        operands.push(left);

        for (span, next_op, next_ast) in operations {
            let pos  = Pos::new(span);
            let p = AST::precedence(next_op.as_str());

            while !operators.is_empty() && operators.last().unwrap().2 <= p {
                let (pos, op, _) = operators.pop().unwrap();
                let r2 = operands.pop().unwrap();
                let r1 = operands.pop().unwrap();

                operands.push(AST::make_op(pos, op, r1, r2))
            }

            operators.push((pos, next_op, p));
            operands.push(next_ast);
        }

        while !operators.is_empty() {
            let (pos, op, _) = operators.pop().unwrap();
            let r2 = operands.pop().unwrap();
            let r1 = operands.pop().unwrap();
            operands.push(AST::make_op(pos, op, r1, r2))
        }

        assert_eq!(operands.len(), 1);
        operands.pop().unwrap()
    }

    fn make_op(pos: Pos, op: Ident, r1: AST, r2: AST) -> AST {
        match op.as_str() {
            "and" | "or" => AST::make_side_effect(pos, op, vec!(r1, r2)),
            _ => 
            //AST::Infix(pos, op, Box::new(r1), Box::new(r2))
            AST::make_invoke(pos.clone(), AST::make_ident(pos, op), vec!(r1, r2))
            //AST::make_invoke(pos, AST::make_access(pos, r1, op), vec!(r2))
        }
    }
}


named!(infix(Span) -> AST,
    do_parse!(
        left: atomic >>
        operations: many0!(tuple!(position!(), infix_op, atomic)) >>
        (AST::fold_op(left, operations))
    )
);

named!(atomic(Span) -> AST,
    wscom!(alt_complete!(
        prefix |
        subatomic_operation
    ))
);

named!(prefix_op(Span) -> Ident,
    map_ident!(alt!(
        tag_word!("not") |
        tag!("~")
    ))
);

named!(prefix(Span) -> AST,
    do_parse!(
        pos: position!() >>
        op: prefix_op >>
        a: atomic >>
        //(AST::Prefix(op, Box::new(a)))
        (AST::make_invoke(Pos::new(pos), AST::make_ident(Pos::new(pos), op), vec!(a)))
        //(AST::make_invoke(AST::make_access(Pos::new(pos), a, op), vec!()))
    )
);

named!(subatomic_operation(Span) -> AST,
    do_parse!(
        left: subatomic >>
        operations: many0!(alt_complete!(
            map!(delimited!(tag!("["), tuple!(position!(), expression), tag!("]")), |(p, e)| SubOP::Index(Pos::new(p), e)) |
            map!(delimited!(tag!("("), tuple!(position!(), expression_list), tag!(")")), |(p, e)| SubOP::Invoke(Pos::new(p), e)) |
            map!(preceded!(tag!("."), tuple!(position!(), identifier)), |(p, s)| SubOP::Accessor(Pos::new(p), s)) |
            map!(preceded!(tag!("::"), tuple!(position!(), identifier)), |(p, s)| SubOP::Resolver(Pos::new(p), s))
        )) >>
        (AST::fold_access(left, operations))
    )
);

enum SubOP {
    Index(Pos, AST),
    Invoke(Pos, Vec<AST>),
    Accessor(Pos, Ident),
    Resolver(Pos, Ident),
}

impl AST {
    fn fold_access(left: AST, operations: Vec<SubOP>) -> Self {
        let mut ret = left;
        for op in operations {
            match op {
                SubOP::Index(p, e) => ret = AST::make_index(p, ret, e),
                SubOP::Invoke(p, e) => ret = AST::make_invoke(p, ret, e),
                SubOP::Accessor(p, name) => ret = AST::make_access(p, ret, name.clone()),
                SubOP::Resolver(p, name) => ret = AST::make_resolve(p, ret, name.clone()),
            }
        }
        ret
    }
}

named!(subatomic(Span) -> AST,
    alt_complete!(
        delimited!(tag!("("), wscom!(expression), tag!(")")) |
        block |
        literal |
        identifier_node
    )
);

named!(expression_list(Span) -> Vec<AST>,
    separated_list_complete!(tag!(","), expression)
);

named!(identifier_node(Span) -> AST,
    do_parse!(
        pos: position!() >>
        i: identifier >>
        (AST::make_ident(Pos::new(pos), i))
    )
);

named!(identifier(Span) -> Ident,
    do_parse!(
        not!(reserved) >>
        s: recognize!(preceded!(
            take_while1!(is_alpha_underscore),
            take_while!(is_alphanumeric_underscore)
        )) >>
        (Ident::from_span(s))
    )
);

named!(class_spec(Span) -> ClassSpec,
    do_parse!(
        pos: position!() >>
        i: identifier >>
        p: opt!(complete!(delimited!(tag!("<"), separated_list_complete!(wscom!(tag!(",")), type_description), tag!(">")))) >>
        (ClassSpec::new(Pos::new(pos), i, p.unwrap_or(vec!())))
    )
);

named!(identifier_typed(Span) -> (Pos, Ident, Option<Type>),
    wscom!(do_parse!(
        pos: position!() >>
        i: identifier >>
        t: opt!(preceded!(wscom!(tag!(":")), type_description)) >>
        (Pos::new(pos), i, t)
    ))
);

named!(symbol_name(Span) -> Ident,
    wscom!(do_parse!(
        s: recognize!(take_while!(is_not_colon)) >>
        (Ident::from_span(s))
    ))
);


named!(any_op(Span) -> Ident,
    alt!(infix_op | prefix_op | map_ident!(alt!(tag!("[]") | tag!("::"))))
);

pub fn parse_type(s: &str) -> Option<Type> {
    match type_description(Span::new(CompleteByteSlice(s.as_bytes()))) {
        Ok((_, t)) => Some(t),
        e => panic!("Error Parsing Type: {:?}   [{:?}]", s, e)
    }
}

named!(type_description(Span) -> Type,
    alt_complete!(
        type_function |
        type_tuple |
        type_variable |
        type_object
    )
);

named!(type_object(Span) -> Type,
    do_parse!(
        cs: class_spec >>
        (Type::Object(cs.ident.name.clone(), UniqueID(0), cs.types.clone()))
    )
);

named!(type_variable(Span) -> Type,
    map!(preceded!(tag!("'"), identifier), |s| Type::Variable(s.name.clone(), UniqueID(0)))
);

named!(type_tuple(Span) -> Type,
    wscom!(do_parse!(
        types: delimited!(tag!("("), separated_list_complete!(wscom!(tag!(",")), type_description), tag!(")")) >>
        (Type::Tuple(types))
    ))
);

named!(type_function(Span) -> Type,
    wscom!(do_parse!(
        //args: delimited!(tag!("("), separated_list_complete!(wscom!(tag!(",")), type_description), tag!(")")) >>
        args: alt_complete!(type_tuple | type_variable | type_object) >>
        wscom!(tag!("->")) >>
        ret: type_description >>
        abi: abi_specifier >>
        (Type::Function(Box::new(args), Box::new(ret), abi))
    ))
);

//named!(type_overload(Span) -> Type,
//    map!(
//        delimited!(tag!("Overload["), wscom!(separated_list_complete!(wscom!(tag!(",")), type_description)), tag!("]")),
//        |t| Type::Overload(t)
//    )
//);

named!(abi_specifier(Span) -> ABI,
    map!(
        opt!(complete!(preceded!(wscom!(tag!("/")), identifier))),
        |s| ABI::from_ident(&s)
    )
);


named!(reserved(Span) -> Span,
    alt!(
        tag_word!("do") |
        tag_word!("end") |
        tag_word!("while") |
        tag_word!("class") |
        tag_word!("import") |
        tag_word!("let") |
        tag_word!("type") |
        tag_word!("match") |
        tag_word!("if") | tag_word!("then") | tag_word!("else") |
        tag_word!("try") | tag_word!("with") | tag_word!("raise") |
        tag_word!("for") | tag_word!("in") |
        tag_word!("fn") | tag_word!("decl")
    )
);



named!(literal(Span) -> AST,
    alt_complete!(
        nil |
        boolean |
        string |
        number |
        // TODO currently a parse error
        tuple |
        list
    )
);

named!(nil(Span) -> AST,
    value!(AST::make_nil(), tag_word!("nil"))
);

named!(boolean(Span) -> AST,
    alt!(
        value!(AST::make_lit(Literal::Boolean(true)), tag_word!("true")) |
        value!(AST::make_lit(Literal::Boolean(false)), tag_word!("false"))
    )
);

named!(string_contents(Span) -> AST,
    map!(
        escaped_transform!(is_not!("\"\\"), '\\',
            alt!(
                tag!("\\") => { |_| &b"\\"[..] } |
                tag!("\"") => { |_| &b"\""[..] } |
                tag!("n")  => { |_| &b"\n"[..] } |
                tag!("r")  => { |_| &b"\r"[..] } |
                tag!("t")  => { |_| &b"\t"[..] }
            )
        ),
        |s| AST::make_lit(Literal::String(String::from_utf8_lossy(&s).into_owned()))
    )
);

named!(string(Span) -> AST,
    delimited!(
        tag!("\""),
        alt!(
            string_contents |
            value!(AST::make_lit(Literal::String(String::new())), tag!(""))
        ),
        tag!("\"")
    )
);


named!(number(Span) -> AST,
    alt_complete!(
        value!(AST::make_lit(Literal::Real(std::f64::NEG_INFINITY)), tag_word!("-Inf")) |
        value!(AST::make_lit(Literal::Real(std::f64::INFINITY)), tag_word!("Inf")) |
        value!(AST::make_lit(Literal::Real(std::f64::NAN)), tag_word!("NaN")) |
        oct_number |
        hex_number |
        int_or_float_number
    )
);

named!(hex_number(Span) -> AST,
    map!(
        preceded!(tag!("0x"), hex_digit),
        |s| AST::make_lit(Literal::Integer(isize::from_str_radix(str::from_utf8(&s.fragment).unwrap(), 16).unwrap()))
    )
);

named!(oct_number(Span) -> AST,
    map!(
        preceded!(tag!("0"), oct_digit),
        |s| AST::make_lit(Literal::Integer(isize::from_str_radix(str::from_utf8(&s.fragment).unwrap(), 8).unwrap()))
    )
);

named!(int_or_float_number(Span) -> AST,
    map!(
        recognize!(
            tuple!(
               opt!(tag!("-")),
               digit,
               opt!(complete!(preceded!(tag!("."), digit)))
               //opt!(complete!(float_exponent))
            )
        ),
        |s| AST::number_from_utf8(&s.fragment)
    )
);

impl AST {
    fn number_from_utf8(s : &[u8]) -> Self {
        let n = str::from_utf8(s).unwrap();
        if let Ok(i) = isize::from_str_radix(n, 10) {
            AST::make_lit(Literal::Integer(i))
        } else {
            AST::make_lit(Literal::Real(f64::from_str(n).unwrap()))
        }
    }
}

named!(tuple(Span) -> AST,
    do_parse!(
        pos: position!() >>
        l: delimited!(
            wscom!(tag!("(")),
            separated_list_complete!(wscom!(tag!(",")), expression),
            wscom!(tag!(")"))
        ) >>
        (AST::make_tuple(Pos::new(pos), l))
    )
);

named!(list(Span) -> AST,
    do_parse!(
        pos: position!() >>
        l: delimited!(
            wscom!(tag!("[")),
            separated_list_complete!(wscom!(tag!(",")), expression),
            wscom!(tag!("]"))
        ) >>
        (AST::make_list(Pos::new(pos), l))
    )
);




named!(separator(Span) -> Span,
    recognize!(many0!(
        //alt!(take_while1!(is_ws) | comment)
        //alt!(take_while1!(is_ws) | delimited!(tag!("//"), not_line_ending, line_ending))
        //terminated!(sp!(alt!(line_comment | block_comment)), line_ending)
        delimited!(space_comment, alt!(line_ending | tag!(";")), multispace_comment)
    ))
);



named!(space_comment(Span) -> Span,
    recognize!(many0!(alt!(line_comment | block_comment | space)))
);

named!(multispace_comment(Span) -> Span,
//map!(
    recognize!(many0!(alt!(line_comment | block_comment | multispace)))
//    |s| { count_lines(s.fragment); s }
//)
);

named!(line_comment(Span) -> Span,
    delimited!(tag!("//"), not_line_ending, peek!(line_ending))    //, |s| AST::Comment(String::from(str::from_utf8(s).unwrap())))
);

// TODO allow for nested comments
named!(block_comment(Span) -> Span,
    delimited!(tag!("/*"), take_until!("*/"), tag!("*/"))              //, |s| AST::Comment(String::from(str::from_utf8(s).unwrap())))
);

/*
macro_rules! named_method {
    ($name:ident( $i:ty ) -> $o:ty, $submac:ident!( $($args:tt)* )) => (
        fn $name<'a>( &self, i: $i ) -> IResult<$i, $o, u32> {
            $submac!(i, $($args)*)
        }
    );
}

struct Test(u32);

impl Test {
    fn test<'a>(&self, i: Span<'a>) -> IResult<Span<'a>, Span<'a>> {
        tag!(i, "Hey")
    }

    named_method!(test2(Span<'a>) -> Span<'a>,
        tag!("Hey")
    );
}
*/

named!(alphanumeric_underscore(Span) -> Span,
    take_while1!(is_alphanumeric_underscore)
);

pub fn is_alpha_underscore(ch: u8) -> bool {
    ch == b'_' || is_alphabetic(ch)
}

pub fn is_alphanumeric_underscore(ch: u8) -> bool {
    ch == b'_' || is_alphanumeric(ch)
}

pub fn is_not_colon(ch: u8) -> bool {
    ch != b':' && !is_space(ch)
}



pub fn print_error(name: &str, span: Span, err: nom::Err<Span, u32>) {
    match err {
        nom::Err::Incomplete(_) => println!("ParseError: incomplete input..."),
        nom::Err::Error(Context::Code(span, code)) |
        nom::Err::Failure(Context::Code(span, code)) => {
            let snippet = span_to_string(span);
            let index = snippet.find('\n').unwrap_or(snippet.len());
            println!("\x1B[1;31m{}:{}:{}: ParseError ({:?}) near {:?}\x1B[0m", name, span.line, span.get_utf8_column(), code, snippet.get(..index).unwrap());
        },
    }
}


