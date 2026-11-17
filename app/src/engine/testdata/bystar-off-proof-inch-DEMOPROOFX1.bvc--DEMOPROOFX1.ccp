<?xml version="1.0" encoding="utf-8"?>
<CuttingControlProgram MeasurementSystem="Imperial" Thickness="0.118110236">
  <Technology />
  <ApplicationFlat PlanWidth="31.496062992" PlanHeight="20.68503937" SheetWidth="39.37007874" SheetHeight="19.68503937" />
  <SubPrograms>
    <SubProgram Id="1" Name="1" PartNumber="1" PartWidth="7.874015748" PartHeight="3.937007874">
      <ControlCommands>
        <SetOperationMode Id="2" Value="3" />
        <SetSensingSensor Id="3" Value="On" />
        <SetToolCorrection Id="4" Value="Right" />
        <MoveLineTo Id="5" X="-0.094488189" Y="0.181102362" />
        <BeginLeadIn Id="6" />
        <SetPiercingMode Id="7" Value="Fine" />
        <ExecPiercing Id="8" />
        <LineTo Id="9" X="0.181102362" Y="0.181102362" />
        <EndLeadIn Id="10" />
        <BeginCutOut Id="11" Contour="Exterior" />
        <LineTo Id="12" X="4.118110236" Y="0.181102362" />
        <ExecMicrojoint Id="13" />
        <LineTo Id="14" X="8.05511811" Y="0.181102362" />
        <LineTo Id="15" X="8.05511811" Y="2.149606299" />
        <ExecMicrojoint Id="16" />
        <LineTo Id="17" X="8.05511811" Y="4.118110236" />
        <LineTo Id="18" X="4.118110236" Y="4.118110236" />
        <ExecMicrojoint Id="19" />
        <LineTo Id="20" X="0.181102362" Y="4.118110236" />
        <LineTo Id="21" X="0.181102362" Y="2.149606299" />
        <ExecMicrojoint Id="22" />
        <LineTo Id="23" X="0.181102362" Y="0.181102362" />
        <BeginLeadOut Id="24" />
        <LineTo Id="25" X="0.181102362" Y="-0.015748031" />
        <EndLeadOut Id="26" />
        <EndCutOut Id="27" />
        <SetToolCorrection Id="28" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="29" Name="2" PartNumber="3" PartWidth="9.448818898" PartHeight="6.299212598">
      <ControlCommands>
        <SetOperationMode Id="30" Value="2" />
        <SetToolCorrection Id="31" Value="Right" />
        <MoveLineTo Id="32" X="4.039370079" Y="0.377952756" />
        <BeginCutOut Id="33" Contour="Unknown" />
        <LineTo Id="34" X="4.039370079" Y="0.850393701" />
        <LineTo Id="35" X="4.275590551" Y="0.850393701" />
        <LineTo Id="36" X="4.354330709" Y="0.771653543" />
        <LineTo Id="37" X="4.354330709" Y="0.456692913" />
        <LineTo Id="38" X="4.275590551" Y="0.377952756" />
        <LineTo Id="39" X="4.039370079" Y="0.377952756" />
        <EndCutOut Id="40" />
        <SetToolCorrection Id="41" Value="None" />
        <SetToolCorrection Id="42" Value="Right" />
        <MoveLineTo Id="43" X="4.826771654" Y="0.377952756" />
        <BeginCutOut Id="44" Contour="Unknown" />
        <LineTo Id="45" X="4.511811024" Y="0.377952756" />
        <LineTo Id="46" X="4.511811024" Y="0.850393701" />
        <LineTo Id="47" X="4.826771654" Y="0.850393701" />
        <EndCutOut Id="48" />
        <SetToolCorrection Id="49" Value="None" />
        <SetToolCorrection Id="50" Value="Right" />
        <MoveLineTo Id="51" X="4.511811024" Y="0.614173228" />
        <BeginCutOut Id="52" Contour="Unknown" />
        <LineTo Id="53" X="4.748031496" Y="0.614173228" />
        <EndCutOut Id="54" />
        <SetToolCorrection Id="55" Value="None" />
        <SetToolCorrection Id="56" Value="Right" />
        <MoveLineTo Id="57" X="4.984251969" Y="0.377952756" />
        <BeginCutOut Id="58" Contour="Unknown" />
        <LineTo Id="59" X="4.984251969" Y="0.850393701" />
        <LineTo Id="60" X="5.141732283" Y="0.614173228" />
        <LineTo Id="61" X="5.299212598" Y="0.850393701" />
        <LineTo Id="62" X="5.299212598" Y="0.377952756" />
        <EndCutOut Id="63" />
        <SetToolCorrection Id="64" Value="None" />
        <SetToolCorrection Id="65" Value="Right" />
        <MoveLineTo Id="66" X="5.535433071" Y="0.377952756" />
        <BeginCutOut Id="67" Contour="Unknown" />
        <LineTo Id="68" X="5.456692913" Y="0.456692913" />
        <LineTo Id="69" X="5.456692913" Y="0.771653543" />
        <LineTo Id="70" X="5.535433071" Y="0.850393701" />
        <LineTo Id="71" X="5.692913386" Y="0.850393701" />
        <LineTo Id="72" X="5.771653543" Y="0.771653543" />
        <LineTo Id="73" X="5.771653543" Y="0.456692913" />
        <LineTo Id="74" X="5.692913386" Y="0.377952756" />
        <LineTo Id="75" X="5.535433071" Y="0.377952756" />
        <EndCutOut Id="76" />
        <SetToolCorrection Id="77" Value="None" />
        <SetOperationMode Id="78" Value="3" />
        <SetToolCorrection Id="79" Value="Left" />
        <MoveLineTo Id="80" X="1.326906477" Y="1.075497563" />
        <BeginLeadIn Id="81" />
        <SetPiercingMode Id="82" Value="Fine" />
        <ExecPiercing Id="83" />
        <LineTo Id="84" X="1.278688175" Y="1.027279261" />
        <ArcTo Id="85" X="1.278688175" Y="0.971601561" SweepAngle="90" />
        <EndLeadIn Id="86" />
        <BeginCutOut Id="87" Contour="Interior" />
        <ArcTo Id="88" X="1.253085095" Y="1.00991928" SweepAngle="337.5" />
        <LineTo Id="89" X="1.278688175" Y="0.971601561" />
        <BeginLeadOut Id="90" />
        <LineTo Id="91" X="1.295055688" Y="0.960665139" />
        <ArcTo Id="92" X="1.349955298" Y="0.969940862" SweepAngle="90" />
        <LineTo Id="93" X="1.389466743" Y="1.025518329" />
        <EndLeadOut Id="94" />
        <EndCutOut Id="95" />
        <SetToolCorrection Id="96" Value="None" />
        <SetOperationMode Id="97" Value="4" />
        <SetToolCorrection Id="98" Value="Left" />
        <MoveLineTo Id="99" X="1.303374313" Y="5.026091738" />
        <BeginLeadIn Id="100" />
        <SetPiercingMode Id="101" Value="Fine" />
        <ExecPiercing Id="102" />
        <LineTo Id="103" X="1.223010476" Y="4.945727901" />
        <ArcTo Id="104" X="1.223010476" Y="4.852931736" SweepAngle="90" />
        <EndLeadIn Id="105" />
        <BeginCutOut Id="106" Contour="Interior" />
        <ArcTo Id="107" X="1.180338675" Y="4.9167946" SweepAngle="337.5" />
        <LineTo Id="108" X="1.223010476" Y="4.852931736" />
        <BeginLeadOut Id="109" />
        <LineTo Id="110" X="1.239377988" Y="4.841995314" />
        <ArcTo Id="111" X="1.33171335" Y="4.851231622" SweepAngle="90" />
        <LineTo Id="112" X="1.403679241" Y="4.939195268" />
        <EndLeadOut Id="113" />
        <EndCutOut Id="114" />
        <SetToolCorrection Id="115" Value="None" />
        <SetToolCorrection Id="116" Value="Left" />
        <MoveLineTo Id="117" X="8.389988486" Y="5.026091738" />
        <BeginLeadIn Id="118" />
        <SetPiercingMode Id="119" Value="Fine" />
        <ExecPiercing Id="120" />
        <LineTo Id="121" X="8.309624649" Y="4.945727901" />
        <ArcTo Id="122" X="8.309624649" Y="4.852931736" SweepAngle="90" />
        <EndLeadIn Id="123" />
        <BeginCutOut Id="124" Contour="Interior" />
        <ArcTo Id="125" X="8.266952848" Y="4.9167946" SweepAngle="337.5" />
        <LineTo Id="126" X="8.309624649" Y="4.852931736" />
        <BeginLeadOut Id="127" />
        <LineTo Id="128" X="8.325992161" Y="4.841995314" />
        <ArcTo Id="129" X="8.418327523" Y="4.851231622" SweepAngle="90" />
        <LineTo Id="130" X="8.490293414" Y="4.939195268" />
        <EndLeadOut Id="131" />
        <EndCutOut Id="132" />
        <SetToolCorrection Id="133" Value="None" />
        <SetOperationMode Id="134" Value="3" />
        <SetToolCorrection Id="135" Value="Left" />
        <MoveLineTo Id="136" X="8.413520651" Y="1.075497563" />
        <BeginLeadIn Id="137" />
        <SetPiercingMode Id="138" Value="Fine" />
        <ExecPiercing Id="139" />
        <LineTo Id="140" X="8.365302349" Y="1.027279261" />
        <ArcTo Id="141" X="8.365302349" Y="0.971601561" SweepAngle="90" />
        <EndLeadIn Id="142" />
        <BeginCutOut Id="143" Contour="Interior" />
        <ArcTo Id="144" X="8.339699268" Y="1.00991928" SweepAngle="337.5" />
        <LineTo Id="145" X="8.365302349" Y="0.971601561" />
        <BeginLeadOut Id="146" />
        <LineTo Id="147" X="8.381669861" Y="0.960665139" />
        <ArcTo Id="148" X="8.436569471" Y="0.969940862" SweepAngle="90" />
        <LineTo Id="149" X="8.476080917" Y="1.025518329" />
        <EndLeadOut Id="150" />
        <EndCutOut Id="151" />
        <SetToolCorrection Id="152" Value="None" />
        <SetToolCorrection Id="153" Value="Right" />
        <MoveLineTo Id="154" X="-0.094488189" Y="-0.125984252" />
        <BeginLeadIn Id="155" />
        <SetPiercingMode Id="156" Value="Fine" />
        <ExecPiercing Id="157" />
        <LineTo Id="158" X="0.181102362" Y="-0.125984252" />
        <EndLeadIn Id="159" />
        <BeginCutOut Id="160" Contour="Exterior" />
        <LineTo Id="161" X="2.280839895" Y="-0.125984252" />
        <ExecMicrojoint Id="162" />
        <LineTo Id="163" X="7.530183727" Y="-0.125984252" />
        <ExecMicrojoint Id="164" />
        <LineTo Id="165" X="9.62992126" Y="-0.125984252" />
        <LineTo Id="166" X="9.62992126" Y="3.023622047" />
        <ExecMicrojoint Id="167" />
        <LineTo Id="168" X="9.62992126" Y="6.173228346" />
        <LineTo Id="169" X="7.530183727" Y="6.173228346" />
        <ExecMicrojoint Id="170" />
        <LineTo Id="171" X="2.280839895" Y="6.173228346" />
        <ExecMicrojoint Id="172" />
        <LineTo Id="173" X="0.181102362" Y="6.173228346" />
        <LineTo Id="174" X="0.181102362" Y="3.023622047" />
        <ExecMicrojoint Id="175" />
        <LineTo Id="176" X="0.181102362" Y="-0.125984252" />
        <BeginLeadOut Id="177" />
        <LineTo Id="178" X="0.181102362" Y="-0.322834646" />
        <EndLeadOut Id="179" />
        <EndCutOut Id="180" />
        <SetToolCorrection Id="181" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="182" Name="3" PartNumber="2" PartWidth="5.905511811" PartHeight="5.905511811">
      <ControlCommands>
        <SetOperationMode Id="183" Value="3" />
        <SetToolCorrection Id="184" Value="Left" />
        <MoveLineTo Id="185" X="2.334277096" Y="3.122003997" />
        <BeginLeadIn Id="186" />
        <SetPiercingMode Id="187" Value="Fine" />
        <ExecPiercing Id="188" />
        <LineTo Id="189" X="2.13937627" Y="2.927160933" />
        <ArcTo Id="190" X="2.139314382" Y="2.509578192" SweepAngle="90" />
        <EndLeadIn Id="191" />
        <BeginCutOut Id="192" Contour="Interior" />
        <LineTo Id="193" X="2.229949633" Y="2.426953244" />
        <ArcTo Id="194" X="2.109145644" Y="2.537080631" SweepAngle="349.411764706" />
        <LineTo Id="195" X="2.139314382" Y="2.509578192" />
        <BeginLeadOut Id="196" />
        <LineTo Id="197" X="2.153861801" Y="2.496316467" />
        <ArcTo Id="198" X="2.460014093" Y="2.503095985" SweepAngle="90" />
        <LineTo Id="199" X="2.596092627" Y="2.645337716" />
        <EndLeadOut Id="200" />
        <EndCutOut Id="201" />
        <SetToolCorrection Id="202" Value="None" />
        <SetToolCorrection Id="203" Value="Right" />
        <MoveLineTo Id="204" X="0.480993856" Y="0.851072596" />
        <BeginLeadIn Id="205" />
        <SetPiercingMode Id="206" Value="Fine" />
        <ExecPiercing Id="207" />
        <LineTo Id="208" X="0.675865804" Y="1.045944544" />
        <EndLeadIn Id="209" />
        <BeginCutOut Id="210" Contour="Exterior" />
        <ArcTo Id="211" X="0.481268346" Y="1.260649751" SweepAngle="354.375" />
        <LineTo Id="212" X="0.675865804" Y="1.045944544" />
        <BeginLeadOut Id="213" />
        <LineTo Id="214" X="0.690451456" Y="1.032724879" />
        <LineTo Id="215" X="0.552187422" Y="0.892606592" />
        <EndLeadOut Id="216" />
        <EndCutOut Id="217" />
        <SetToolCorrection Id="218" Value="None" />
      </ControlCommands>
    </SubProgram>
    <SubProgram Id="219" Name="ResidualSheetCuts" PartWidth="0" PartHeight="21.68503937">
      <ControlCommands>
        <SetOperationMode Id="220" Value="0" />
        <SetToolCorrection Id="221" Value="None" />
        <SetToolCorrection Id="222" Value="Right" />
        <MoveLineTo Id="223" X="31.496062992" Y="3" />
        <BeginCutOut Id="224" Contour="Exterior" />
        <SetPiercingMode Id="225" Value="Fine" />
        <ExecPiercing Id="226" />
        <LineTo Id="227" X="31.496062992" Y="2" />
        <SetSensingSensor Id="228" Value="Off" />
        <LineTo Id="229" X="31.496062992" Y="0" />
        <SetSensingSensor Id="230" Value="On" />
        <EndCutOut Id="231" />
        <ExecHighPositioning Id="232" />
        <MoveLineTo Id="233" X="31.496062992" Y="3" />
        <BeginCutOut Id="234" Contour="Exterior" />
        <SetPiercingMode Id="235" Value="NoTime" />
        <ExecPiercing Id="236" />
        <LineTo Id="237" X="31.496062992" Y="4" />
        <LineTo Id="238" X="31.496062992" Y="19.68503937" />
        <SetSensingSensor Id="239" Value="Off" />
        <LineTo Id="240" X="31.496062992" Y="21.68503937" />
        <SetSensingSensor Id="241" Value="On" />
        <EndCutOut Id="242" />
        <ExecHighPositioning Id="243" />
        <SetToolCorrection Id="244" Value="None" />
      </ControlCommands>
    </SubProgram>
  </SubPrograms>
  <ControlCommands>
    <SetClamps Id="245" Value="Off" />
    <CallSubProgram Id="246" SubProgramName="1" X="1" Y="1" Rotation="0" />
    <CallSubProgram Id="247" SubProgramName="2" X="1" Y="8" Rotation="0" />
    <CallSubProgram Id="248" SubProgramName="3" X="12" Y="1" Rotation="0" />
    <CallSubProgram Id="249" SubProgramName="ResidualSheetCuts" X="0" Y="-1" Rotation="0" />
  </ControlCommands>
</CuttingControlProgram>
