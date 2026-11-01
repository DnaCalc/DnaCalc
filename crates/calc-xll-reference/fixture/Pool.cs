// SPDX-License-Identifier: MIT
using System;
using System.Collections.Generic;
using ExcelDna.Integration;

public class PoolProbe : IExcelAddIn
{
    public void AutoOpen()
    {
        var delegates = new List<Delegate>();
        var attributes = new List<object>();
        var arguments = new List<List<object>>();
        for (int i = 0; i < 10000; ++i)
        {
            int offset = i;
            delegates.Add(new Func<double, double>(value => value + offset));
            attributes.Add(new ExcelFunctionAttribute { Name = "X2.Pool" + i });
            arguments.Add(new List<object> { new ExcelArgumentAttribute { Name = "value" } });
        }
        ExcelIntegration.RegisterDelegates(delegates, attributes, arguments);
    }
    public void AutoClose() { }
}
