module veryl_testcase_Module35;
    logic aa;

    veryl_testcase_Module35B xx (
        .aa   (aa),
        .bb   (),
        .bbbb ()
    );
endmodule

module veryl_testcase_Module35B (
    input int unsigned aa  ,
    input int unsigned bb  ,
    input int unsigned bbbb
);
endmodule
