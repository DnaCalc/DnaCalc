// SPDX-License-Identifier: MIT
using ExcelDna.Integration;

public static class HelloFunctions
{
    [ExcelFunction(Name="SayHello")]
    public static string SayHello(string name) => "Hello, " + name;
}
